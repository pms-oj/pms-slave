use async_compression::futures::bufread::BrotliDecoder;
use async_std::fs::DirBuilder;
use async_std::io::BufReader;
use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::task::spawn;
use async_tar::Archive;

use bincode::Options;

use async_std::task::sleep;
use futures::FutureExt;
use futures::{join, select};
use judge_protocol::handshake::*;
use judge_protocol::judge::*;
use judge_protocol::packet::*;
use judge_protocol::security::*;
use k256::ecdh::EphemeralSecret;
use k256::ecdh::SharedSecret;
use k256::PublicKey;
use rand::thread_rng;
use tempfile::NamedTempFile;

use async_std::channel::{unbounded, Receiver, Sender};
use async_std::sync::*;

use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use crate::constants::*;
use crate::container::*;
use crate::judge::*;
use crate::language::{compile_with_graders, CompileResult};
use crate::timer::*;
use crate::{CONFIG, LANGUAGES, MASTER_PASS};
use log::{error, info, trace};
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum Actions {
    Reconnect(u64),
    Shutdown,
    Unknown,
}

struct State {
    key: Arc<EphemeralSecret>,
    locked: Mutex<bool>,
    node_id: Mutex<u32>,
    shared: Arc<Mutex<Option<SharedSecret>>>,
    signal: Arc<Mutex<Sender<Actions>>>,
    judge: Arc<Mutex<Option<OnJudge>>>,
}

impl State {
    async fn verify_token(&mut self, stream: Arc<TcpStream>) -> async_std::io::Result<()> {
        let body = BodyAfterHandshake::<()> {
            node_id: (*self.node_id.lock().await),
            client_pubkey: self.key.public_key(),
            req: (),
        };
        let packet = Packet::make_packet(Command::VerifyToken, body.bytes());
        packet.send(Arc::clone(&stream)).await
    }

    async fn update_judge(
        &self,
        stream: Arc<TcpStream>,
        uuid: Uuid,
        state: JudgeState,
    ) -> async_std::io::Result<()> {
        let body = BodyAfterHandshake {
            node_id: *self.node_id.lock().await,
            client_pubkey: self.key.public_key(),
            req: JudgeResponseBody {
                uuid,
                result: state,
            },
        };
        let packet = Packet::make_packet(
            Command::GetJudgeStateUpdate,
            bincode::DefaultOptions::new()
                .with_big_endian()
                .with_fixint_encoding()
                .serialize(&body)
                .unwrap(),
        );
        packet.send(Arc::clone(&stream)).await
    }

    async fn handle_command(&mut self, stream: Arc<TcpStream>, packet: Packet) {
        match packet.heady.header.command {
            Command::Handshake => {
                if let Ok(res) = bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .deserialize::<HandshakeResponse>(&packet.heady.body)
                {
                    match res.result {
                        HandshakeResult::Success => {
                            self.node_id = Mutex::new(res.node_id.unwrap());
                            self.shared = Arc::new(Mutex::new(Some(
                                self.key.diffie_hellman(&res.server_pubkey.unwrap()),
                            )));
                            info!(
                                "Handshake was established from remote {}",
                                stream.peer_addr().unwrap()
                            );
                        }
                        HandshakeResult::PasswordNotMatched => {
                            error!("Master password is not matched. Trying to shutdown ...");
                            self.signal.lock().await.send(Actions::Shutdown).await.ok();
                        }
                        _ => {
                            error!("Unknown detected");
                        }
                    }
                } else {
                    error!("An error occurred on processing Command::Handshake. Trying to reconnect in {} secs ...", SLEEP_TIME);
                    self.signal
                        .lock()
                        .await
                        .send(Actions::Reconnect(SLEEP_TIME))
                        .await
                        .ok();
                }
            }
            Command::ReqVerifyToken => {
                if let Ok(state) = bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .deserialize::<bool>(&packet.heady.body)
                {
                    if !state {
                        info!("Session was expired. Trying to reconnect ...");
                        self.signal.lock().await.send(Actions::Reconnect(0)).await;
                    } else {
                        info!("Command::VerifyToken was succeed");
                    }
                } else {
                    error!("An error occurred on processing Command::ReqVerifyToken. Trying to reconnect in {} secs ...", SLEEP_TIME);
                    self.signal
                        .lock()
                        .await
                        .send(Actions::Reconnect(SLEEP_TIME))
                        .await
                        .ok();
                }
            }
            Command::TestCaseEnd => {
                trace!("end judge");
                *self.judge.lock().await = None;
                *self.locked.lock().await = false;
            }
            Command::TestCaseUpdate => {
                if let Ok(test) = bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .deserialize::<TestCaseUpdateBody>(&packet.heady.body)
                {
                    if let Some(onjudge) = self.judge.lock().await.as_ref() {
                        if onjudge.uuid == test.uuid {
                            if *self.locked.lock().await {
                                if let Some(shared_key) = self.shared.lock().await.as_ref() {
                                    let key = expand_key(shared_key);
                                    let (stdin, stdout_origin) =
                                        (test.stdin.decrypt(&key), test.stdout.decrypt(&key));
                                    let run_tempdir = tempfile::tempdir().unwrap();
                                    let (stdin_p, stdout_origin_p, stdout_p) = (
                                        onjudge.tempdir.path().join(STDIN_FILE_NAME),
                                        onjudge.tempdir.path().join(STDOUT_ORIGIN_FILE_NAME),
                                        onjudge.tempdir.path().join(STDOUT_FILE_NAME),
                                    );
                                    let (mut stdin_f, mut stdout_origin_f, mut stdout_f) = (
                                        std::fs::File::create(stdin_p.clone()).unwrap(),
                                        std::fs::File::create(stdout_origin_p.clone()).unwrap(),
                                        std::fs::File::create(stdout_p.clone()).unwrap(),
                                    );
                                    use std::os::unix::fs::PermissionsExt;
                                    stdout_f.flush().ok();
                                    std::fs::set_permissions(
                                        onjudge.tempdir.path().to_path_buf(),
                                        std::fs::Permissions::from_mode(0o777),
                                    )
                                    .ok();
                                    std::fs::set_permissions(
                                        stdout_p.clone(),
                                        std::fs::Permissions::from_mode(0o777),
                                    )
                                    .ok();
                                    stdin_f.write_all(&stdin).ok();
                                    stdin_f.flush().ok();
                                    stdout_origin_f.write_all(&stdout_origin).ok();
                                    stdout_origin_f.flush().ok();
                                    if let (Some(manager_lang), Some(object_path)) = (
                                        onjudge.manager_lang.clone(),
                                        onjudge.object_binary.clone(),
                                    ) {
                                        // 'Novel' mode
                                        let run = Runv2 {
                                            temp_path: onjudge.tempdir.path().to_path_buf(),
                                            object_path: object_path,
                                            box_dir: run_tempdir,
                                            main_lang: onjudge.main_lang.clone(),
                                            manager_lang: manager_lang,
                                            time_limit: (onjudge.time_limit as f64)
                                                * CONVERT_TO_SECONDS,
                                            mem_limit: onjudge.mem_limit,
                                        };
                                        let res = run.run();
                                        debug!(
                                            "(Judge: {}) (Test: {}) {:?}",
                                            onjudge.uuid,
                                            test.uuid,
                                            res.meta.clone()
                                        );
                                        if let Some(status) = res.meta.status {
                                            // Failed?
                                            match status {
                                                RunStatus::TimedOut => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::TimeLimitExceed(test.test_uuid),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                                RunStatus::DiedOnSignal => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::DiedOnSignal(
                                                            test.test_uuid,
                                                            res.meta.exitsig.unwrap(),
                                                        ),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                                RunStatus::InternalErr => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::InternalError(test.test_uuid),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                                RunStatus::RuntimeErr => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::RuntimeError(
                                                            test.test_uuid,
                                                            res.meta.exitcode.unwrap(),
                                                        ),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                                _ => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::UnknownError,
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                            }
                                        } else {
                                            // Success
                                            // Let's check stdout by checker
                                            let dir_checker = tempfile::tempdir().unwrap();
                                            let checker = CheckerRun {
                                                checker_lang: onjudge.checker_lang.clone(),
                                                temp_path: onjudge.tempdir.path().to_path_buf(),
                                                box_dir: dir_checker,
                                            };
                                            let res_checker = checker.run();
                                            debug!(
                                                "(Checker) (Judge: {}) (Test: {}) {:?}",
                                                onjudge.uuid,
                                                test.uuid,
                                                res_checker.meta.clone()
                                            );
                                            if let Some(status_checker) = res_checker.meta.status {
                                                self.update_judge(
                                                    stream,
                                                    test.uuid,
                                                    JudgeState::WrongAnswer(
                                                        test.test_uuid,
                                                        (res_checker.meta.time.unwrap()
                                                            * CONVERT_TO_MILLISECS)
                                                            as u64,
                                                        res_checker.meta.cg_mem.unwrap(),
                                                    ),
                                                )
                                                .await
                                                .ok();
                                            } else {
                                                self.update_judge(
                                                    stream,
                                                    test.uuid,
                                                    JudgeState::Accepted(
                                                        test.test_uuid,
                                                        (res_checker.meta.time.unwrap()
                                                            * CONVERT_TO_MILLISECS)
                                                            as u64,
                                                        res_checker.meta.cg_mem.unwrap(),
                                                    ),
                                                )
                                                .await
                                                .ok();
                                            }
                                        }
                                    } else {
                                        // 'Simple' mode
                                        let run = Run {
                                            temp_path: onjudge.tempdir.path().to_path_buf(),
                                            box_dir: run_tempdir,
                                            language: onjudge.main_lang.clone(),
                                            time_limit: (onjudge.time_limit as f64)
                                                * CONVERT_TO_SECONDS,
                                            mem_limit: onjudge.mem_limit,
                                        };
                                        let res = run.run();
                                        trace!("{:?}", res.meta.clone());
                                        if let Some(status) = res.meta.status {
                                            // Failed?
                                            match status {
                                                RunStatus::TimedOut => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::TimeLimitExceed(test.test_uuid),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                                RunStatus::DiedOnSignal => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::DiedOnSignal(
                                                            test.test_uuid,
                                                            res.meta.exitsig.unwrap(),
                                                        ),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                                RunStatus::InternalErr => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::InternalError(test.test_uuid),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                                RunStatus::RuntimeErr => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::RuntimeError(
                                                            test.test_uuid,
                                                            res.meta.exitcode.unwrap(),
                                                        ),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                                _ => {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::UnknownError,
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                            }
                                        } else {
                                            // Success
                                            // Let's check stdout by checker
                                            let dir_checker = tempfile::tempdir().unwrap();
                                            let checker = CheckerRun {
                                                checker_lang: onjudge.checker_lang.clone(),
                                                temp_path: onjudge.tempdir.path().to_path_buf(),
                                                box_dir: dir_checker,
                                            };
                                            let res_checker = checker.run();
                                            trace!("{:?}", res_checker.meta);
                                            if let Some(status_checker) = res_checker.meta.status {
                                                self.update_judge(
                                                    stream,
                                                    test.uuid,
                                                    JudgeState::WrongAnswer(
                                                        test.test_uuid,
                                                        (res_checker.meta.time.unwrap()
                                                            * CONVERT_TO_MILLISECS)
                                                            as u64,
                                                        res_checker.meta.cg_mem.unwrap(),
                                                    ),
                                                )
                                                .await
                                                .ok();
                                            } else {
                                                if let Some(score) = res_checker.score {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::Complete(
                                                            test.test_uuid,
                                                            score,
                                                            (res_checker.meta.time.unwrap()
                                                                * CONVERT_TO_MILLISECS)
                                                                as u64,
                                                            res_checker.meta.cg_mem.unwrap(),
                                                        ),
                                                    )
                                                    .await
                                                    .ok();
                                                } else {
                                                    self.update_judge(
                                                        stream,
                                                        test.uuid,
                                                        JudgeState::Accepted(
                                                            test.test_uuid,
                                                            (res_checker.meta.time.unwrap()
                                                                * CONVERT_TO_MILLISECS)
                                                                as u64,
                                                            res_checker.meta.cg_mem.unwrap(),
                                                        ),
                                                    )
                                                    .await
                                                    .ok();
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                error!("Unable to handle Command::TestCaseUpdate (JudgeState::UnlockedSlave)");
                                self.update_judge(stream, test.uuid, JudgeState::UnlockedSlave)
                                    .await
                                    .ok();
                            }
                        } else {
                            error!("Unable to handle Command::TestCaseUpdate (JudgeState::JudgeNotFound)");
                            self.update_judge(stream, test.uuid, JudgeState::JudgeNotFound)
                                .await
                                .ok();
                        }
                    } else {
                        error!(
                            "Unable to handle Command::TestCaseUpdate (JudgeState::JudgeNotFound)"
                        );
                        self.update_judge(stream, test.uuid, JudgeState::JudgeNotFound)
                            .await
                            .ok();
                    }
                }
            }
            Command::GetJudgev2 => {
                if let Ok(judge_req) = bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .deserialize::<JudgeRequestBodyv2>(&packet.heady.body)
                {
                    info!("Got a new judgement (v2) request: {}", judge_req.uuid);
                    if !(*self.locked.lock().await) {
                        if let (Some(checker_lang), Some(main_lang), Some(manager_lang)) = (
                            LANGUAGES.get(judge_req.checker_lang),
                            LANGUAGES.get(judge_req.main_lang),
                            LANGUAGES.get(judge_req.manager_lang),
                        ) {
                            if let Some(shared_key) = self.shared.lock().await.as_ref() {
                                *self.locked.lock().await = true;
                                let key = expand_key(shared_key);
                                let checker_code = judge_req.checker_code.decrypt(&key);
                                let main_code = judge_req.main_code.decrypt(&key);
                                let manager_code = judge_req.manager_code.decrypt(&key);
                                let graders_buf = judge_req.graders.decrypt(&key);
                                let graders_decoder = BrotliDecoder::new(graders_buf.as_slice());
                                let decoded = graders_decoder.into_inner();
                                let graders_ar = Archive::new(decoded);
                                let dir = tempfile::tempdir().unwrap();
                                //dbg!(std::process::Command::new("ls -la").current_dir(dir.path()).output().unwrap().stdout);
                                graders_ar.unpack(dir.path()).await.ok();
                                self.update_judge(
                                    Arc::clone(&stream),
                                    judge_req.uuid,
                                    JudgeState::DoCompile,
                                )
                                .await
                                .ok();
                                let c_path = dir.path().join(CHECKER_NAME);
                                let m_path = dir.path().join(MANAGER_NAME);
                                let o_path = dir
                                    .path()
                                    .join(GRADERS_PATH)
                                    .join(judge_req.object_path.clone());
                                let graders_path = dir.path().join(GRADERS_PATH);
                                let b_compile = compile_with_graders(
                                    graders_path,
                                    main_code,
                                    judge_req.main_path,
                                );
                                let c_compile = checker_lang.compile(checker_code, c_path.clone());
                                let m_compile = manager_lang.compile(manager_code, m_path.clone());
                                let (b_res, c_res, m_res) = join!(b_compile, c_compile, m_compile);
                                match b_res {
                                    CompileResult::Error(stderr) => {
                                        trace!("Unable to compile main code: {}", stderr);
                                        self.update_judge(
                                            Arc::clone(&stream),
                                            judge_req.uuid,
                                            JudgeState::CompileError(stderr),
                                        )
                                        .await
                                        .ok();
                                        *self.locked.lock().await = false;
                                    }
                                    CompileResult::Success(stdout) => {
                                        if let (
                                            CompileResult::Success(_),
                                            CompileResult::Success(_),
                                        ) = (c_res.clone(), m_res.clone())
                                        {
                                            if !o_path.exists() {
                                                trace!("o_path is not exists");
                                                self.update_judge(
                                                    Arc::clone(&stream),
                                                    judge_req.uuid,
                                                    JudgeState::GeneralError(String::from(
                                                        "o_path is not exists",
                                                    )),
                                                )
                                                .await
                                                .ok();
                                                *self.locked.lock().await = false;
                                            } else {
                                                *self.judge.lock().await = Some(OnJudge {
                                                    uuid: judge_req.uuid,
                                                    main_lang: main_lang.clone(),
                                                    checker_lang: checker_lang.clone(),
                                                    manager_lang: Some(manager_lang.clone()),
                                                    main_binary: o_path,
                                                    checker_binary: c_path,
                                                    object_binary: Some(judge_req.object_path),
                                                    time_limit: judge_req.time_limit,
                                                    mem_limit: judge_req.mem_limit,
                                                    tempdir: dir,
                                                });
                                                self.update_judge(
                                                    Arc::clone(&stream),
                                                    judge_req.uuid,
                                                    JudgeState::CompleteCompile(stdout),
                                                )
                                                .await
                                                .ok();
                                            }
                                        } else {
                                            error!(
                                                "Checker or manager compile failed: {:?}, {:?}",
                                                c_res, m_res
                                            );
                                            self.update_judge(
                                                Arc::clone(&stream),
                                                judge_req.uuid,
                                                JudgeState::GeneralError(String::from(
                                                    "Checker or manager compile failed",
                                                )),
                                            )
                                            .await
                                            .ok();
                                            *self.locked.lock().await = false;
                                        }
                                    }
                                }
                            }
                        } else {
                            error!("Unable to get judgement languages");
                            self.update_judge(
                                Arc::clone(&stream),
                                judge_req.uuid,
                                JudgeState::LanguageNotFound,
                            )
                            .await
                            .ok();
                            *self.locked.lock().await = false;
                        }
                    } else {
                        error!("Unable to handle Command::GetJudgev2 (JudgeState::LockedSlave)");
                        self.update_judge(
                            Arc::clone(&stream),
                            judge_req.uuid,
                            JudgeState::LockedSlave,
                        )
                        .await
                        .ok();
                        *self.locked.lock().await = false;
                    }
                }
            }
            Command::GetJudge => {
                if let Ok(judge_req) = bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .deserialize::<JudgeRequestBody>(&packet.heady.body)
                {
                    info!("Got a new judgement request: {}", judge_req.uuid);
                    if !(*self.locked.lock().await) {
                        if let Some(checker_lang) = LANGUAGES.get(judge_req.checker_lang.clone()) {
                            if let Some(main_lang) = LANGUAGES.get(judge_req.main_lang.clone()) {
                                if let Some(shared_key) = self.shared.lock().await.as_ref() {
                                    let key = expand_key(shared_key);
                                    let checker_code = judge_req.checker_code.decrypt(&key);
                                    let main_code = judge_req.main_code.decrypt(&key);
                                    *self.locked.lock().await = true;
                                    self.update_judge(
                                        Arc::clone(&stream),
                                        judge_req.uuid,
                                        JudgeState::DoCompile,
                                    )
                                    .await
                                    .ok();
                                    let dir = tempfile::tempdir().unwrap();
                                    let c_path = dir.path().join(CHECKER_NAME);
                                    let m_path = dir.path().join(BINARY_NAME);
                                    let c_res = checker_lang.compile(checker_code, c_path.clone());
                                    let m_res = main_lang.compile(main_code, m_path.clone());
                                    if let CompileResult::Error(stderr) = c_res.await {
                                        trace!("Unable to compile checker code: {}", stderr);
                                        self.update_judge(
                                            Arc::clone(&stream),
                                            judge_req.uuid,
                                            JudgeState::GeneralError(stderr),
                                        )
                                        .await
                                        .ok();
                                        *self.locked.lock().await = false;
                                    } else {
                                        match m_res.await {
                                            CompileResult::Error(stderr) => {
                                                trace!("Unable to compile main code: {}", stderr);
                                                self.update_judge(
                                                    Arc::clone(&stream),
                                                    judge_req.uuid,
                                                    JudgeState::CompileError(stderr),
                                                )
                                                .await
                                                .ok();
                                                *self.locked.lock().await = false;
                                            }
                                            CompileResult::Success(stdout) => {
                                                use std::os::unix::fs::PermissionsExt;
                                                std::fs::set_permissions(
                                                    dir.path().to_path_buf(),
                                                    std::fs::Permissions::from_mode(0o777),
                                                )
                                                .ok();
                                                *self.judge.lock().await = Some(OnJudge {
                                                    uuid: judge_req.uuid,
                                                    main_lang: main_lang.clone(),
                                                    checker_lang: checker_lang.clone(),
                                                    manager_lang: None,
                                                    main_binary: m_path,
                                                    checker_binary: c_path,
                                                    object_binary: None,
                                                    time_limit: judge_req.time_limit,
                                                    mem_limit: judge_req.mem_limit,
                                                    tempdir: dir,
                                                });
                                                self.update_judge(
                                                    Arc::clone(&stream),
                                                    judge_req.uuid,
                                                    JudgeState::CompleteCompile(stdout),
                                                )
                                                .await
                                                .ok();
                                            }
                                        }
                                    }
                                } else {
                                    error!("Command::Handshake must be satisfied first");
                                    self.update_judge(
                                        Arc::clone(&stream),
                                        judge_req.uuid,
                                        JudgeState::GeneralError(String::new()),
                                    )
                                    .await
                                    .ok();
                                    *self.locked.lock().await = false;
                                }
                            } else {
                                error!(
                                    "Unable to get main code language {}",
                                    judge_req.main_lang.clone()
                                );
                                self.update_judge(
                                    Arc::clone(&stream),
                                    judge_req.uuid,
                                    JudgeState::LanguageNotFound,
                                )
                                .await
                                .ok();
                            }
                        } else {
                            error!(
                                "Unable to get checker code language {}",
                                judge_req.checker_lang.clone()
                            );
                            self.update_judge(
                                Arc::clone(&stream),
                                judge_req.uuid,
                                JudgeState::LanguageNotFound,
                            )
                            .await
                            .ok();
                        }
                    } else {
                        error!("Unable to handle Command::GetJudge (JudgeState::LockedSlave)");
                        self.update_judge(
                            Arc::clone(&stream),
                            judge_req.uuid,
                            JudgeState::LockedSlave,
                        )
                        .await
                        .ok();
                    }
                }
            }
            _ => {
                error!("An unknown command has received");
                // Unknown
            }
        }
    }
}

pub async fn open_protocol() {
    loop {
        let mut shutdown = false;
        // do master connection loop
        if let Ok(stream) = TcpStream::connect(CONFIG.host.master.clone()).await {
            let stream: Arc<TcpStream> = Arc::new(stream);
            let key = EphemeralSecret::random(thread_rng());
            let (send, mut recv): (Sender<Actions>, Receiver<Actions>) = unbounded();
            let state = Arc::new(Mutex::new(State {
                key: Arc::new(key),
                locked: Mutex::new(false),
                node_id: Mutex::new(std::u32::MAX),
                shared: Arc::new(Mutex::new(None)),
                signal: Arc::new(Mutex::new(send.clone())),
                judge: Arc::new(Mutex::new(None)),
            }));
            let handshake_req = HandshakeRequest {
                client_pubkey: state.lock().await.key.public_key(),
                pass: MASTER_PASS.clone(),
            };
            // Send Handshake packet
            let handshake = Packet::make_packet(
                Command::Handshake,
                bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .serialize(&handshake_req)
                    .unwrap(),
            );
            handshake.send(Arc::clone(&stream)).await.ok();
            {
                let send_cloned = send.clone();
                let stream_cloned = Arc::clone(&stream);
                spawn(async move { check_alive(send_cloned, stream_cloned).await });
            }
            //sleep(Duration::from_secs(1)).await;
            loop {
                select! {
                    actions = recv.next().fuse() => match actions {
                        Some(action) => match action {
                            Actions::Reconnect(secs) => {
                                sleep(Duration::from_secs(secs)).await;
                                break;
                            }
                            Actions::Shutdown => {
                                shutdown = true;
                                break;
                            }
                            _ => {}
                        },
                        None => {}
                    },
                packet = Packet::from_stream(Arc::clone(&stream)).fuse() => match packet {
                    Ok(packet) => {
                        let stream_cloned = Arc::clone(&stream);
                        let state_mutex = Arc::clone(&state);
                        spawn(async move {
                            state_mutex
                                .lock()
                                .await
                                .handle_command(Arc::clone(&stream_cloned), packet)
                                .await
                        });
                    },
                    Err(err) => {
                        error!("Got a packet error: {:?}", err);
                        break;
                    }
                }
                }
            }
            drop(state);
            drop(recv);
        } else {
            error!(
                "Cannot connect to server {:?}. Trying to connect in {} secs ...",
                CONFIG.host.master.clone(),
                SLEEP_TIME
            );
            sleep(Duration::from_secs(SLEEP_TIME)).await;
        }
        if shutdown {
            info!("Actions::Shutdown was triggered");
            break;
        }
    }
}

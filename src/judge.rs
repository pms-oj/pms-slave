use async_std::net::TcpStream;
use bincode::Options;
use judge_protocol::judge::*;
use judge_protocol::packet::*;
use std::pin::Pin;

pub async fn update_judge(
    mut stream: &mut TcpStream,
    state: JudgeState,
) -> async_std::io::Result<()> {
    let body: JudgeResponseBody = JudgeResponseBody { result: state };
    let packet = Packet::make_packet(
        Command::GetJudgeStateUpdate,
        bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding()
            .serialize::<JudgeResponseBody>(&body)
            .unwrap(),
    );
    packet.send(Pin::new(&mut stream)).await
}

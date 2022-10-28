FROM oraclelinux:9-slim

RUN microdnf upgrade -y && \
    microdnf install make g++ git libcap-devel nano curl -y && \
    microdnf --enablerepo=ol9_codeready_builder install libstdc++-static glibc-static -y && \
    git clone https://github.com/polymath-cc/isolate.git /opt/isolate

RUN mkdir -p /opt/rust /opt/pms-slave /app /usr/share/testlib

WORKDIR /opt/isolate
RUN make install

WORKDIR /opt/rust
RUN curl https://sh.rustup.rs -s >> rustup.sh
RUN chmod 755 /opt/rust/rustup.sh
RUN ./rustup.sh -y

ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /opt/pms-slave
COPY dummy.rs .
COPY Cargo.toml .
RUN sed -i 's#src/main.rs#dummy.rs#' Cargo.toml
RUN cargo build --release
RUN sed -i 's#dummy.rs#src/main.rs#' Cargo.toml
COPY . .
RUN cargo install --path .

WORKDIR /app
RUN cp -r /opt/pms-slave/langs /app/langs
RUN cp /opt/pms-slave/config.example.toml /app/config.toml
RUN cp /opt/pms-slave/log4rs.example.yaml /app/log4rs.yaml
RUN cp /opt/pms-slave/assets/testlib/testlib.h /usr/share/testlib/testlib.h
RUN cp /opt/pms-slave/assets/scripts/run.judge.sh /app/run.judge.sh
RUN cp /opt/pms-slave/assets/scripts/checker.sh /app/checker.sh
RUN rm -rf /opt/pms-slave /opt/rust /opt/isolate

ENTRYPOINT ["pms-slave"]
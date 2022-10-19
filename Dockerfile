FROM oraclelinux:9-slim as build

COPY . /opt/pms-slave

RUN microdnf upgrade -y && \
    microdnf install make g++ git libcap-devel nano curl -y && \
    git clone https://github.com/polymath-cc/isolate.git /opt/isolate

RUN mkdir -p /opt/rust /app

WORKDIR /opt/isolate
RUN make isolate

WORKDIR /opt/rust
RUN curl https://sh.rustup.rs -s >> rustup.sh
RUN chmod 755 /opt/rust/rustup.sh
RUN ./rustup.sh -y

ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /opt/pms-slave
RUN cargo build --release

FROM oraclelinux:9-slim

RUN microdnf upgrade -y && \
    microdnf install make g++ git libcap-devel nano curl -y && \
    mkdir -p /usr/share/testlib

COPY --from=build /opt/isolate /opt/isolate

WORKDIR /opt/isolate
RUN make install

WORKDIR /app
COPY --from=build /opt/pms-slave/target/release/pms-slave /usr/bin
COPY --from=build /opt/pms-slave/langs /app/langs
COPY --from=build /opt/pms-slave/config.example.toml /app/config.toml
COPY --from=build /opt/pms-slave/log4rs.example.yaml /app/log4rs.yaml
COPY --from=build /opt/pms-slave/assets/testlib/testlib.h /usr/share/testlib/testlib.h
ENTRYPOINT ["pms-slave"]
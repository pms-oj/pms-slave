FROM ubuntu:latest as build

COPY . /opt/pms-slave

RUN apt-get update && \
    apt-get install apt-transport-https ca-certificates -y && \
    update-ca-certificates && \
    apt-get install g++ git libcap-dev build-essential nano curl -y && \
    git clone https://github.com/polymath-cc/isolate.git /opt/isolate

RUN mkdir -p /opt/rust /app /work

WORKDIR /opt/isolate
RUN make isolate

WORKDIR /opt/rust
RUN curl https://sh.rustup.rs -s >> rustup.sh
RUN chmod 755 /opt/rust/rustup.sh
RUN ./rustup.sh -y

ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /opt/pms-slave
RUN cargo build --release

FROM ubuntu:latest

RUN apt-get update && \
    apt-get install apt-transport-https ca-certificates -y && \
    update-ca-certificates && \
    apt-get install g++ gcc python3 python2 rustc libcap-dev build-essential -y && \
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
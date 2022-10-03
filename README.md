# pms-slave

Just run following commands

```Bash
$ docker build . -t pms-slave:local
$ docker run -it --privileged pms-slave:local -p 3030:3030 -v ./config.toml:/app/config.toml
```
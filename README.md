# pms-slave

## Test

Just run following commands (Example)

```Bash
$ docker build . -t pms-slave:local
$ docker run -v /home/ubuntu/pms-slave/config.toml:/app/config.toml -it --privileged pms-slave:local
```

## TODO

# pms-slave

## Test

Just run following commands (Example)

```Bash
$ docker build . -t pms-slave:local
$ docker run -v /home/ubuntu/pms-slave/config.toml:/app/config.toml -it --privileged pms-slave:local
```

file logging path is `${PWD}/log/pms-slave.log` for default

## TODO

## LICENSE

- [testlib](https://github.com/MikeMirzayanov/testlib) by MikeMirzayanov in [MIT License](https://opensource.org/licenses/MIT)
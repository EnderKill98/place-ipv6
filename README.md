TODO:
 - [ ] Grayscale is way too dark. Try with gamma correction / powing/sqrting!
 - [ ] Multiple clients is way to inefficient. Maybe let each handle one frame, or make the sending more parallel (they probably io block each other too much)

Not actually sending any ipv6 but using one (or more) TCP Connections, telnet style!

- Website: https://c3pixelflut.de/
- Client software: https://github.com/sbernauer/breakwater


to build:

```
cargo build --release
```

see help with:

```
target/release/place-ipv6 -h
```

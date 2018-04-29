# deptyr - a super cheap way to spawn tty programs under process supervision

Say you want to run a program that sometimes crashes (hello,
rtorrent!) under a process supervisor (hello, daemontools!), but that
program uses terminal graphics or other cute things that are great if
you're running a thing interactively, but a complete pain if you want
reasonable log files. Deptyr can help you!

This tool has two modes: The first mode (`run`) allocates a PTY,
redirects IO to that PTY and then execs a program, and the other mode
(`interact`) receives that PTY from a unix domain socket and proxies
IO to/from the program.

If you run daemontools or another service supervision tool, each of
`run` and `interact` should be a separate service.

# Installation

`deptyr` is written in Rust, and should compile with all versions of
Rust from 1.23.0 onwards. To build `deptyr`, clone this repo and run
`cargo build --release` in the root. Once everything is done, you
should see a binary in `target/release/deptyr`.

# Example usage

In this example, we'll run a de-pty'ed rtorrent under daemontools in
freebsd, and spawn a head under a screen.

To spawn the main program, add this `run` script under the rtorrent service:

``` sh
#!/bin/sh

exec deptyr -s /tmp/deptyr-rtorrent.socket run -- rtorrent
```

And to spawn the screen, add this to your `/etc/rc.local`:

``` sh
screen -d -m deptyr -s /tmp/deptyr-rtorrent.socket interact
```

# Etymology & thanks

Deptyr owes a lot (almost all) of its code and its motivation &
existence to [reptyr](http://github.com/nelhage/reptyr) by Nelson
Elhage. So, you know, I replaced the "r" with a "d".

Pronunciation-wise, you can say it "de-p-t-y-er".

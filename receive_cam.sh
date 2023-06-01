#!/bin/sh

target='udp://[::0]:5000/stream'

#ffplay -i $target -vf scale=256:256 -acodec none
#ffmpeg -i $target -vf scale=256:256 -acodec none -pix_fmt rgb24 -f rawvideo pipe:1 -loglevel quiet | pv >/dev/null
ffmpeg -i $target -vf scale=256:256 -acodec none -pix_fmt rgb24 -f rawvideo pipe:1 -loglevel quiet | ./place-ipv6 -i eth0 -s 2a01:4ff:1f0:8d4e::1 -d d2:74:7f:6e:37:e3 -p 500000

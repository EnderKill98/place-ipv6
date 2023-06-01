#!/bin/sh

target='udp://[::1]:5000/stream'

ffmpeg -f v4l2 -framerate 25 -video_size 512x512 -i /dev/video0 -vcodec libx264 -tune zerolatency -b 900k -acodec none -f mpegts $target

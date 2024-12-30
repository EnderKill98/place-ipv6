#export X=100 Y=300 WIDTH=1140 HEIGHT=640
#export X=100 Y=300 WIDTH=860 HEIGHT=480
#export X=0 Y=150 WIDTH=640 HEIGHT=360
#export X=100 Y=300 WIDTH=1140 HEIGHT=640
#export X=20 Y=150 WIDTH=640 HEIGHT=360

#export X=2320 Y=0 WIDTH=640 HEIGHT=360
#export X=2320 Y=0 WIDTH=853 HEIGHT=480
#export X=2320 Y=0 WIDTH=920 HEIGHT=520
#export X=2320 Y=0 WIDTH=920 HEIGHT=520
#export X=2320 Y=0 WIDTH=1062 HEIGHT=600
export X=600 Y=600 WIDTH=852 HEIGHT=480
#export X=0 Y=0 WIDTH=1920 HEIGHT=1080
#export X=3110 Y=0 WIDTH=480 HEIGHT=360
#export X=2950 Y=0 WIDTH=640 HEIGHT=480
#export X=1920 Y=0 WIDTH=1920 HEIGHT=1080

export ADDR=table.apokalypse.email:1337
#export ADDR=wall.c3pixelflut.de:1337
#export ADDR=localhost:1337

exec ffmpeg -i "$1" -vf scale=$WIDTH:$HEIGHT:force_original_aspect_ratio=decrease,pad=$WIDTH:$HEIGHT:-1:-1:color=black,realtime -acodec none -pix_fmt rgb24 -f rawvideo pipe:1 -loglevel quiet \
  | RUST_BACKTRACE=1 ./target/release/place-ipv6 -c -n -d $ADDR -x $X -y $Y raw-pipe-stdin $WIDTH $HEIGHT -c 1

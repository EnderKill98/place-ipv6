#export X=100 Y=300 WIDTH=1140 HEIGHT=640
#export X=100 Y=300 WIDTH=860 HEIGHT=480
#export X=100 Y=300 WIDTH=1140 HEIGHT=640
#export X=0 Y=0 WIDTH=1920 HEIGHT=1080
#export X=3000 Y=500 WIDTH=640 HEIGHT=360
#export X=0 Y=150 WIDTH=640 HEIGHT=360

export X=20 Y=150 WIDTH=480 HEIGHT=360 # 4:3

export ADDR=table.apokalypse.email:1337
#export ADDR=wall.c3pixelflut.de:1337
#export ADDR=localhost:1337

exec yt-dlp $1 -o- \
  | ffmpeg -i pipe:0 -vf scale=$WIDTH:$HEIGHT:force_original_aspect_ratio=decrease,pad=$WIDTH:$HEIGHT:-1:-1:color=black,realtime -acodec none -pix_fmt rgb24 -f rawvideo pipe:1 -loglevel quiet \
  | ./target/release/place-ipv6 -d $ADDR -x $X -y $Y -n raw-pipe-stdin $WIDTH $HEIGHT -r 100

# How do I compile?

Compiling the Program
To compile the program, follow the steps below:

Open a terminal or command prompt.
Navigate to the project directory.
Run the following command:
```bash
gcc lodepng.c -o lodepng.c
```

then compile the program with
```bash
sudo gcc -O3 -finline-functions -flto -funroll-loops -march=native -mcpu=native -falign-loops -ftree-vectorize -ftree-vectorizer-verbose=2 -ffast-math -funswitch-loops -fprefetch-loop-arrays -frename-registers -ftree-loop-distribution -floop-interchange -floop-strip-mine -floop-block -floop-optimize -fomit-frame-pointer -o place place.c -lpthread -static -llodepng
```
This command compiles the program using various optimization flags and the required dependencies.


## Changing the IP Prefix

To change the IP prefix, you need to edit the code at line 56 in the `place.c` file. Open the file and locate the following line:

```c
snprintf(ip, sizeof(ip), "2a01:4f8:c012:f8e6:2%03X:%04X:%02X:%02X%02X",
```

## What does what?

This is the help prompt

```
$ ./place
Usage: ./place [options] <input.png>
Options:
  -t <count>   Number of threads to use (default: 4)
  -l <count>   Number of iterations to process the image (default: 5)
  -help        Display this help menu
```

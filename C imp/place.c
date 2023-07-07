// look if you're confused
// I am too

#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netinet/ip6.h>
#include <netinet/icmp6.h>
#include <arpa/inet.h>
#include <errno.h>
#include "lodepng.h"
#include <pthread.h>



#define MAX_IP_LEN 39

struct ThreadData {
    volatile unsigned char* image;
    unsigned width;
    unsigned start_x;
    unsigned start_y;
    unsigned end_x;
    unsigned end_y;
    int sockfd;
    char packet[sizeof(struct icmp6_hdr) + 8];
};

void* processImagePart(void* arg) {
    struct ThreadData* data = (struct ThreadData*)arg;
    volatile unsigned char* image = data->image;
    unsigned width = data->width;
    unsigned start_x = data->start_x;
    unsigned start_y = data->start_y;
    unsigned end_x = data->end_x;
    unsigned end_y = data->end_y;
    int sockfd = data->sockfd;
    char* packet = data->packet;

    for (unsigned y = start_y; y < end_y; y += 2) {
        for (unsigned x = start_x; x < end_x; x++) {
            unsigned char* pixel = (unsigned char*)&image[4 * (y * width + x)];

            if (pixel[3] == 0) {
                continue;
            }

            unsigned char r = pixel[0];
            unsigned char g = pixel[1];
            unsigned char b = pixel[2];

            char ip[MAX_IP_LEN + 1];
            snprintf(ip, sizeof(ip), "2a01:4f8:c012:f8e6:2%03X:%04X:%02X:%02X%02X",
                     x, y, r, g, b);

            struct in6_addr dst_ip;
            if (inet_pton(AF_INET6, ip, &dst_ip) != 1 && strcmp(ip, "::") != 0) {
                continue;
            }

            struct icmp6_hdr icmp6_hdr = {
                .icmp6_type = ICMP6_ECHO_REQUEST,
                .icmp6_code = 0,
                .icmp6_id = 0,
                .icmp6_seq = 0
            };
            memcpy(packet, &icmp6_hdr, sizeof(icmp6_hdr));
            unsigned char payload[8] = {0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08};
            memcpy(packet + sizeof(struct icmp6_hdr), payload, sizeof(payload));
            struct sockaddr_in6 dst_addr;
            memset(&dst_addr, 0, sizeof(dst_addr));
            dst_addr.sin6_family = AF_INET6;
            dst_addr.sin6_addr = dst_ip;

            int result;
            do {
                result = sendto(sockfd, packet, sizeof(packet), 0, (struct sockaddr*)&dst_addr, sizeof(dst_addr));
                if (result == -1 && errno == ENOBUFS) {
                    // Wait for a short period if "No buffer space available" error occurs
                    usleep(10);
                }
            } while (result == -1 && errno == ENOBUFS);

            if (result == -1) {
                perror("sendto");
                exit(EXIT_FAILURE);
            }
        }
    }

    return NULL;
}

void printHelp(const char* programName) {
    printf("Usage: %s [options] <input.png>\n", programName);
    printf("Options:\n");
    printf("  -t <count>   Number of threads to use (default: 4)\n");
    printf("  -l <count>   Number of iterations to process the image (default: 5)\n");
    printf("  -help        Display this help menu\n");
}

int main(int argc, char* argv[]) {
    unsigned num_sections = 4;
    unsigned num_iterations = 1;
    int opt;
    while ((opt = getopt(argc, argv, "t:l:help")) != -1) {
        switch (opt) {
            case 't':
                num_sections = atoi(optarg);
                break;
            case 'l':
                num_iterations = atoi(optarg);
                break;
            case 'h':
                printHelp(argv[0]);
                return EXIT_SUCCESS;
            default:
                printHelp(argv[0]);
                return EXIT_FAILURE;
        }
    }

    if (argc - optind < 1) {
        printHelp(argv[0]);
        return EXIT_FAILURE;
    }

    char* input_filename = argv[optind];

    for (unsigned iteration = 0; iteration < num_iterations; iteration++) {
        unsigned error;
        unsigned char* image;
        unsigned width, height;
        error = lodepng_decode32_file(&image, &width, &height, input_filename);
        if (error) {
            fprintf(stderr, "Error decoding PNG: %s\n", lodepng_error_text(error));
            exit(EXIT_FAILURE);
        }
        int sockfd = socket(AF_INET6, SOCK_RAW, IPPROTO_ICMPV6);
        if (sockfd == -1) {
            perror("socket");
            exit(EXIT_FAILURE);
        }

        unsigned section_height = height / num_sections;
        unsigned remainder = height % num_sections;
        pthread_t* threads = malloc(num_sections * sizeof(pthread_t));
        struct ThreadData* threadData = malloc(num_sections * sizeof(struct ThreadData));
        char* packet_buffer = malloc(num_sections * (sizeof(struct icmp6_hdr) + 8));
        unsigned start_y = 0;
        unsigned end_y = section_height;
        for (unsigned i = 0; i < num_sections; i++) {
            if (i == num_sections - 1 && remainder != 0) {
                end_y += remainder;
            }

            threadData[i].image = image;
            threadData[i].width = width;
            threadData[i].start_x = 0;
            threadData[i].start_y = start_y;
            threadData[i].end_x = width;
            threadData[i].end_y = end_y;
            threadData[i].sockfd = sockfd;
            memcpy(threadData[i].packet, &packet_buffer[i * (sizeof(struct icmp6_hdr) + 8)], sizeof(threadData[i].packet));
            if (pthread_create(&threads[i], NULL, processImagePart, &threadData[i]) != 0) {
                perror("pthread_create");
                exit(EXIT_FAILURE);
            }

            start_y = end_y;
            end_y += section_height;
        }
        for (unsigned i = 0; i < num_sections; i++) {
            if (pthread_join(threads[i], NULL) != 0) {
                perror("pthread_join");
                exit(EXIT_FAILURE);
            }
        }
        free(threads);
        free(threadData);
        free(packet_buffer);
        free(image);
        close(sockfd);
    }

    return EXIT_SUCCESS;
}

#include <stdio.h>
#include <errno.h>
#include <limits.h>
#include <stdlib.h>

#include <tcp.h>

typedef enum {
  PARSE_OK,
  PARSE_ERR
} parse_res;

parse_res parse_port(const char* input_port, ushort *out) {
  char *end;
  long port = strtol(input_port, &end, 10);
  if (port < 0) {
    return PARSE_ERR;
  }
  if (port > USHRT_MAX) {
    return PARSE_ERR;
  }
  if (*end != '\0') {
    return PARSE_ERR;
  }
  *out = (ushort)port;
  return PARSE_OK;
}

int main(int argc, char *argv[]) {
  if (argc == 1) {
    fprintf(stderr, "Usage: %s <port>\n", argv[0]);
    return 0;
  }

  ushort port;
  if (parse_port(argv[1], &port) != PARSE_OK) {
    fprintf(stderr, "Invalid port: %s", argv[1]);
    return -1;
  }

  tcp_server server;
  auto res = bind_tcp_port(&server, port);
  if (res == -1) {
    return -1;
  }

  printf("Server bound and listening...\n");

  return 0;
}

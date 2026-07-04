#ifndef TCP_H
#define TCP_H

#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>

typedef struct {
  int socket_fd;
  struct sockaddr_in address;
} tcp_server;

int bind_tcp_port(tcp_server *server, ushort port);

#endif

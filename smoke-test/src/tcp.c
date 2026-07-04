#include <stdio.h>
#include <sys/types.h>
#include <sys/socket.h>

#include <tcp.h>

constexpr int backlog = 5;

int bind_tcp_port(tcp_server *server, ushort port) {
  if (server == nullptr) {
    return -1;
  }

  server->socket_fd = socket(PF_INET, SOCK_STREAM, 0);
  if (server->socket_fd == -1) {
    perror("socket");
    return -1;
  }

  server->address.sin_family = AF_INET;
  server->address.sin_addr.s_addr = INADDR_ANY;
  server->address.sin_port = htons(port);

  if (bind(server->socket_fd, (struct sockaddr*) &server->address, sizeof(server->address)) == -1) {
    perror("bind");
    close(server->socket_fd);
    return -1;
  }

  if (listen(server->socket_fd, backlog) == -1) {
    perror("listen");
    close(server->socket_fd);
    return -1;
  }

  return 0;
}

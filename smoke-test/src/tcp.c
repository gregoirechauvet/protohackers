#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <sys/types.h>
#include <sys/socket.h>

#include <tcp.h>

constexpr int BACKLOG = 5;
constexpr int BUFFER_SIZE = 8096;

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

  if (listen(server->socket_fd, BACKLOG) == -1) {
    perror("listen");
    close(server->socket_fd);
    return -1;
  }

  return 0;
}

void teardown(tcp_server *server) {
  if (server == nullptr) {
    return;
  }

  close(server->socket_fd);
}

int accept_client(tcp_server server) {
  struct sockaddr_in client_address = {0};
  socklen_t client_len = 0;

  int client_fd = accept(server.socket_fd, (struct sockaddr*)&client_address, &client_len);
  if (client_fd == -1) {
    perror("accept");
    return -1;
  }

  return client_fd;
}

typedef struct {
  int fd;
  char buffer[BUFFER_SIZE];
} client_t;

void *handle_client(void *arg) {
  client_t *client = arg;

  int read_res, write_res;
  do {
    read_res = read(client->fd, client->buffer, BUFFER_SIZE);
    if (read_res == -1) {
      break;
    }

    printf("Content from client %d: %*.s", client->fd, read_res, client->buffer);

    write_res = write(client->fd, client->buffer, read_res);
    if (write_res == -1) {
      printf("Client %d disconnected", client->fd);
      return nullptr;
    }
  } while(read_res > 0);

  printf("Client %d disconnected", client->fd);
  return nullptr;
}

void spawn_client(int client_fd) {
  pthread_t *thread = malloc(sizeof(pthread_t));
  client_t *client = malloc(sizeof(client_t));
  client->fd = client_fd;

  pthread_create(thread, nullptr, handle_client, client);
  // pthread_detach(*thread);
}

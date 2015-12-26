int create_server(char *socket_path);
int connect_server(char *socket_path);
int recv_file_descriptor(int socket);
int send_file_descriptor(int socket, int fd_to_send);

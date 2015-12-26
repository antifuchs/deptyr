/*
 * Routines to open UNIX domain sockets & send fds along them. Copied
   mostly wholesale from:
 * <http://blog.varunajayasiri.com/passing-file-descriptors-between-processes-using-sendmsg-and-recvmsg>.
 */

#include <unistd.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <strings.h>

#include "deptyr.h"

int create_server(char *socket_path) {
     struct sockaddr_un addr;
     int fd;

     if ((fd = socket(AF_LOCAL, SOCK_STREAM, 0)) < 0) {
          die("Failed to create server socket");
          return fd;
     }

     memset(&addr, 0, sizeof(addr));

     addr.sun_family = AF_LOCAL;
     unlink(socket_path);
     strcpy(addr.sun_path, socket_path);

     if (bind(fd, (struct sockaddr *) &(addr),
              sizeof(addr)) < 0) {
          die("Failed to bind server socket");
          return -1;
     }

     if (listen(fd, 0) < 0) {
          die("Failed to listen on server socket");
          return -1;
     }

     return fd;
}


int connect_server(char *socket_path) {
     struct sockaddr_un addr;
     int fd;

     if ((fd = socket(AF_LOCAL, SOCK_STREAM, 0)) < 0) {
          die("Failed to create client socket");
          return fd;
     }

     memset(&addr, 0, sizeof(addr));

     addr.sun_family = AF_LOCAL;
     strcpy(addr.sun_path, socket_path);

     if (connect(fd,
                 (struct sockaddr *) &(addr),
                 sizeof(addr)) < 0) {
          die("Failed to connect to server");
          return -1;
     }

     return fd;
}


int
recv_file_descriptor(
     int socket) /* Socket from which the file descriptor is read */
{
     int sent_fd;
     struct msghdr message;
     struct iovec iov[1];
     struct cmsghdr *control_message = NULL;
     char ctrl_buf[CMSG_SPACE(sizeof(int))];
     char data[1];
     int res;

     memset(&message, 0, sizeof(struct msghdr));
     memset(ctrl_buf, 0, CMSG_SPACE(sizeof(int)));

     /* For the dummy data */
     iov[0].iov_base = data;
     iov[0].iov_len = sizeof(data);

     message.msg_name = NULL;
     message.msg_namelen = 0;
     message.msg_control = ctrl_buf;
     message.msg_controllen = CMSG_SPACE(sizeof(int));
     message.msg_iov = iov;
     message.msg_iovlen = 1;

     if((res = recvmsg(socket, &message, 0)) <= 0)
          return res;

     /* Iterate through header to find if there is a file descriptor */
     for(control_message = CMSG_FIRSTHDR(&message);
         control_message != NULL;
         control_message = CMSG_NXTHDR(&message,
                                       control_message))
     {
          if( (control_message->cmsg_level == SOL_SOCKET) &&
              (control_message->cmsg_type == SCM_RIGHTS) )
          {
               return *((int *) CMSG_DATA(control_message));
          }
     }

     return -1;
}

int
send_file_descriptor(
     int socket, /* Socket through which the file descriptor is passed */
     int fd_to_send) /* File descriptor to be passed, could be another socket */
{
     struct msghdr message;
     struct iovec iov[1];
     struct cmsghdr *control_message = NULL;
     char ctrl_buf[CMSG_SPACE(sizeof(int))];
     char data[1];

     memset(&message, 0, sizeof(struct msghdr));
     memset(ctrl_buf, 0, CMSG_SPACE(sizeof(int)));

     /* We are passing at least one byte of data so that recvmsg() will not return 0 */
     data[0] = ' ';
     iov[0].iov_base = data;
     iov[0].iov_len = sizeof(data);

     message.msg_name = NULL;
     message.msg_namelen = 0;
     message.msg_iov = iov;
     message.msg_iovlen = 1;
     message.msg_controllen =  CMSG_SPACE(sizeof(int));
     message.msg_control = ctrl_buf;

     control_message = CMSG_FIRSTHDR(&message);
     control_message->cmsg_level = SOL_SOCKET;
     control_message->cmsg_type = SCM_RIGHTS;
     control_message->cmsg_len = CMSG_LEN(sizeof(int));

     *((int *) CMSG_DATA(control_message)) = fd_to_send;

     return sendmsg(socket, &message, 0);
}

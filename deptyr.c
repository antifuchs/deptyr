#include <fcntl.h>
#include <unistd.h>
#include <sys/types.h>
#include <sys/select.h>
#include <sys/ioctl.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <string.h>
#include <stdarg.h>
#include <termios.h>
#include <signal.h>

#include "deptyr.h"
#include "platform/platform.h"

static int verbose = 0;

void _debug(const char *pfx, const char *msg, va_list ap) {

     if (pfx)
          fprintf(stderr, "%s", pfx);
     vfprintf(stderr, msg, ap);
     fprintf(stderr, "\n");
}

void die(const char *msg, ...) {
     va_list ap;
     va_start(ap, msg);
     _debug("[!] ", msg, ap);
     va_end(ap);

     exit(1);
}

void debug(const char *msg, ...) {

     va_list ap;

     if (!verbose)
          return;

     va_start(ap, msg);
     _debug("[+] ", msg, ap);
     va_end(ap);
}

void error(const char *msg, ...) {
     va_list ap;
     va_start(ap, msg);
     _debug("[-] ", msg, ap);
     va_end(ap);
}

void setup_raw(struct termios *save) {
     struct termios set;
     if (tcgetattr(0, save) < 0) {
          fprintf(stderr, "Unable to read terminal attributes: %m");
          return;
     }
     set = *save;
     cfmakeraw(&set);
     if (tcsetattr(0, TCSANOW, &set) < 0)
          die("Unable to set terminal attributes: %m");
}

void resize_pty(int pty) {
     struct winsize sz;
     if (ioctl(0, TIOCGWINSZ, &sz) < 0) {
          // provide fake size to workaround some problems
          struct winsize defaultsize = {30, 80, 640, 480};
          if (ioctl(pty, TIOCSWINSZ, &defaultsize) < 0) {
               fprintf(stderr, "Cannot set terminal size\n");
          }
          return;
     }
     ioctl(pty, TIOCSWINSZ, &sz);
}

int writeall(int fd, const void *buf, ssize_t count) {
     ssize_t rv;
     while (count > 0) {
          rv = write(fd, buf, count);
          if (rv < 0) {
               if (errno == EINTR)
                    continue;
               return rv;
          }
          count -= rv;
          buf += rv;
     }
     return 0;
}

volatile sig_atomic_t winch_happened = 0;

void do_winch(int signal) {
     winch_happened = 1;
}

void do_proxy(int pty) {
     char buf[4096];
     ssize_t count;
     fd_set set;
     struct timeval timeout;
     while (1) {
          if (winch_happened) {
               winch_happened = 0;
               /*
                * FIXME: If a signal comes in after this point but before
                * select(), the resize will be delayed until we get more
                * input. signalfd() is probably the cleanest solution.
                */
               resize_pty(pty);
          }
          FD_ZERO(&set);
          FD_SET(0, &set);
          FD_SET(pty, &set);
          timeout.tv_sec = 0;
          timeout.tv_usec = 1000;
          if (select(pty + 1, &set, NULL, NULL, &timeout) < 0) {
               if (errno == EINTR)
                    continue;
               fprintf(stderr, "select: %m");
               return;
          }
          if (FD_ISSET(0, &set)) {
               count = read(0, buf, sizeof buf);
               if (count < 0)
                    return;
               writeall(pty, buf, count);
          }
          if (FD_ISSET(pty, &set)) {
               count = read(pty, buf, sizeof buf);
               if (count <= 0)
                    return;
               writeall(1, buf, count);
          }
     }
}

void usage(char *me) {
     fprintf(stderr, "Usage: %s CMD\n", me);
}

int main(int argc, char *argv[])
{
     struct termios saved_termios;
     struct sigaction act;
     int pty;
     int opt;
     int err;

     while ((opt = getopt(argc, argv, "hV")) != -1) {
          switch (opt) {
          case 'h':
               usage(argv[0]);
               return 0;
          case 'V':
               verbose = 1;
               break;
          default:
               usage(argv[0]);
               return 1;
          }
     }

     if (optind >= argc) {
          fprintf(stderr, "%s: No command specified\n", argv[0]);
          usage(argv[0]);
          return 1;
     }

     if ((pty = get_pt()) < 0)
          die("Unable to allocate a new pseudo-terminal: %m");
     if (unlockpt(pty) < 0)
          die("Unable to unlockpt: %m");
     if (grantpt(pty) < 0)
          die("Unable to grantpt: %m");

     printf("Opened a new pty: %s\n", ptsname(pty));
     fflush(stdout);

     // TODO: Wait and send the master handle down a socket.
     setenv("REPTYR_PTY", ptsname(pty), 1);
     {
          int f;
          setpgid(0, getppid());
          setsid();
          f = open(ptsname(pty), O_RDONLY, 0);
          dup2(f, 0);
          close(f);
          f = open(ptsname(pty), O_WRONLY, 0);
          dup2(f, 1);
          dup2(f, 2);
          close(f);
     }
     close(pty);
     execvp(argv[2], argv + 2);
     return 1;
}

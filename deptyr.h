#define DEPTYR_VERSION "0.0.1"

#define __printf __attribute__((format(printf, 1, 2)))
void __printf die(const char *msg, ...) __attribute__((noreturn));
void __printf debug(const char *msg, ...);
void __printf error(const char *msg, ...);

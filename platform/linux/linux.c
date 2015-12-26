#ifdef __linux__

#include "linux.h"
#include "../platform.h"
#include "../../reptyr.h"
#include "../../ptrace.h"
#include <stdint.h>

/* Homebrew posix_openpt() */
int get_pt() {
     return open("/dev/ptmx", O_RDWR | O_NOCTTY);
}

#endif

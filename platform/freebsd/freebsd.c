#include <stdlib.h>
#include <sys/stat.h>
#include <fcntl.h>
#include "../platform.h"

int get_pt() {
     return posix_openpt(O_RDWR | O_NOCTTY);
}

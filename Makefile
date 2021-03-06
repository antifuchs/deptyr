OBJS = deptyr.o unix_socket.o

UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
	OBJS += platform/linux/linux.o
	CFLAGS += -DWITH_SYSTEMD
	LDFLAGS += -lsystemd
endif
ifeq ($(UNAME_S),FreeBSD)
	OBJS += platform/freebsd/freebsd.o
	LDFLAGS += -lprocstat
endif
ifeq ($(UNAME_S),Darwin)
	OBJS += platform/freebsd/freebsd.o
#	LDFLAGS += -lprocstat
endif


all: deptyr

deptyr: $(OBJS)
	cc $(OBJS) $(LDFLAGS) -o $@

deptyr.o: deptyr.h unix_socket.h

clean:
	rm -f $(OBJS) deptyr

.PHONY: PHONY all

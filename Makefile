OBJS = deptyr.o

UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
	OBJS += platform/linux/linux.o
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

deptyr.o: deptyr.h

clean:
	rm -f $(OBJS) deptyr

.PHONY: PHONY all

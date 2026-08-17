CC ?= cc
CFLAGS ?= -std=c11 -Wall -Wextra -O2
SRC = $(wildcard src/*.c)
BIN = bcvm

.PHONY: all trace clean
all: $(BIN)

$(BIN): $(SRC)
	$(CC) $(CFLAGS) -o $@ $(SRC) -lm

# Build with per-instruction execution tracing available (pass --trace at runtime)
trace: CFLAGS += -DBCVM_TRACE
trace: clean all

clean:
	rm -f $(BIN)

/* Minimal packaged POSIX group leader. Rust retains this exact Child until final
 * group signaling and reap; std spawns the opaque target Command into its group.
 * No user command, environment reconstruction, fork or process enumeration here. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <signal.h>
#include <unistd.h>
int main(void) {
    if (getpgrp() != getpid()) return 125;
    if (signal(SIGTERM, SIG_IGN) == SIG_ERR) return 125;
    const char ready[] = "R1\n";
    unsigned long offset = 0;
    while (offset < sizeof(ready) - 1) {
        ssize_t n = write(STDOUT_FILENO, ready + offset, sizeof(ready) - 1 - offset);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) return 125;
        offset += (unsigned long)n;
    }
    if (close(STDOUT_FILENO) != 0) return 125;
    for (;;) pause();
}

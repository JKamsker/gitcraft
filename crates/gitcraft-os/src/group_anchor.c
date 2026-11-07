/* A single-threaded, packaged POSIX group leader. No process enumeration or signal
 * selectors. Rust owns this leader until final group termination and reaping.
 * The user's exit status travels on a private fd; the live leader never exits first. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

static void write_all(int fd, const void *data, size_t len) {
    const char *p = data;
    while (len) {
        ssize_t n = write(fd, p, len);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) _exit(125);
        p += n; len -= (size_t)n;
    }
}
static void report(int fd, char kind, int status) {
    char message[64];
    int n = snprintf(message,sizeof(message),"%c %d\n",kind,status);
    if (n <= 0 || (size_t)n >= sizeof(message)) _exit(125);
    write_all(fd,message,(size_t)n);
}
int main(int argc, char **argv) {
    if (argc < 3) return 125;
    char *end = NULL;
    long number = strtol(argv[1],&end,10);
    if (!end || *end || number < 3 || number > INT_MAX) return 125;
    int control = (int)number;
    if (fcntl(control,F_SETFD,FD_CLOEXEC) < 0) return 125;
    /* The app established this group before exec. A live anchor avoids Darwin's
     * zombie-only killpg EPERM without ignoring permission errors. */
    if (getpgrp() != getpid()) return 125;
    if (signal(SIGTERM,SIG_IGN) == SIG_ERR) return 125;
    int failed[2];
    if (pipe(failed) < 0) { report(control,'E',errno); return 125; }
    if (fcntl(failed[1],F_SETFD,FD_CLOEXEC) < 0) { report(control,'E',errno); return 125; }
    pid_t child = fork();
    if (child < 0) { report(control,'E',errno); return 125; }
    if (child == 0) {
        close(failed[0]); close(control);
        if (signal(SIGTERM,SIG_DFL) == SIG_ERR) _exit(125);
        execvp(argv[2],argv+2);
        int error = errno;
        write_all(failed[1],&error,sizeof(error));
        _exit(127);
    }
    close(failed[1]);
    int error = 0;
    ssize_t got;
    do { got = read(failed[0],&error,sizeof(error)); } while (got < 0 && errno == EINTR);
    close(failed[0]);
    int status;
    pid_t waited;
    do { waited = waitpid(child,&status,0); } while (waited < 0 && errno == EINTR);
    if (waited != child) report(control,'E',errno);
    else if (got > 0) report(control,'E',error);
    else if (got < 0) report(control,'E',EIO);
    else report(control,'S',status);
    close(control); close(STDIN_FILENO); close(STDOUT_FILENO); close(STDERR_FILENO);
    for (;;) pause();
}

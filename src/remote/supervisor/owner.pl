# gitcraft owned execution protocol v1. Perl POSIX/core-module fallback.
use strict;
use warnings;
use POSIX qw(setsid _exit);
use IO::Socket::UNIX;
use IO::Select;
use JSON::PP;
use MIME::Base64 qw(decode_base64);
use Fcntl qw(O_RDONLY O_WRONLY O_RDWR O_CREAT O_EXCL O_NOFOLLOW F_GETFD F_SETFD FD_CLOEXEC);
use Time::HiRes qw(time sleep);
use Errno qw(ESRCH EAGAIN EWOULDBLOCK);
use Socket qw(SOCK_STREAM);
$SIG{PIPE} = 'IGNORE';
my $MAX = 64 << 20;
my $JSON = JSON::PP->new->utf8->canonical;
sub encode { $JSON->encode($_[0]) . "\n" }
sub identity {
    my ($o) = @_;
    die 'invalid owner fields' unless ref($o) eq 'HASH' && join(',',sort keys %$o) eq 'nonce,path,token,v';
    die 'invalid owner identity' unless $o->{v} == 1 && $o->{nonce} =~ /\A[0-9a-f]{1,96}\z/ && $o->{token} =~ /\A[0-9a-f]{64}\z/;
    die 'invalid owner path' unless $o->{path} =~ m{\A/tmp/gitcraft-owned-[0-9a-f]{32}\z};
    return $o;
}
sub random_hex {
    my ($count) = @_;
    open(my $f, '<', '/dev/urandom') or die 'secure randomness unavailable';
    my $bytes = '';
    while (length($bytes) < $count) { my $n = sysread($f, my $part, $count - length($bytes)); die 'random read failed' unless $n; $bytes .= $part; }
    close($f); return unpack('H*', $bytes);
}
sub atomic {
    my ($path, $bytes, $exclusive) = @_;
    my $name = $exclusive ? $path : "$path.new";
    sysopen(my $f, $name, O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW, 0600) or die "private file: $!";
    binmode($f);
    print $f $bytes or die 'private write failed';
    $f->flush or die 'private flush failed';
    $f->sync or die 'private sync failed';
    close($f) or die 'private close failed';
    rename($name, $path) or die 'receipt rename failed' unless $exclusive;
}
sub terminal {
    my ($o) = @_;
    my $f;
    if (!sysopen($f, "$o->{path}/terminal", O_RDONLY|O_NOFOLLOW)) {
        return undef if $!{ENOENT};
        die 'receipt open failed';
    }
    my $bytes = ''; read($f, $bytes, 2049); close($f);
    die 'oversized receipt' if length($bytes) > 2048;
    my $v = $JSON->decode($bytes);
    die 'invalid receipt' unless ref($v) eq 'HASH' && join(',',sort keys %$v) eq 'code,nonce,path,state,token,v';
    for my $k (keys %$o) { die 'unconfirmed receipt' unless $v->{$k} eq $o->{$k}; }
    die 'unconfirmed receipt' unless $v->{state} eq 'stopped' && $v->{code} =~ /\A[0-9]+\z/ && $v->{code} <= 255;
    return $v;
}
sub control {
    my $o = identity($_[0]);
    my $got = terminal($o); return $got if $got;
    local $SIG{ALRM} = sub { die 'owner control timeout' };
    alarm(5);
    my $s = IO::Socket::UNIX->new(Type=>SOCK_STREAM, Peer=>"$o->{path}/control") or die 'owner endpoint unavailable';
    $s->autoflush(1);
    print $s encode({%$o, op=>'stop'}) or die 'control write failed';
    my $bytes = '';
    while (length($bytes) <= 2048 && $bytes !~ /\n\z/) { my $n = sysread($s, my $part, 2049-length($bytes)); last unless $n; $bytes .= $part; }
    close($s); alarm(0);
    my $final = terminal($o); die 'no atomic acknowledgement' unless $final && encode($JSON->decode($bytes)) eq encode($final);
    return $final;
}
sub cloexec { my $fd = fcntl($_[0], F_GETFD, 0); defined($fd) && fcntl($_[0], F_SETFD, $fd|FD_CLOEXEC) or die 'close-on-exec failed'; }
sub retire {
    my ($owner,$ops)=@_; identity($owner);
    $ops //= {
        receipt=>sub { terminal($_[0]) or die 'missing atomic retirement receipt'; },
        unlink=>sub { my ($path)=@_; unlink($path) or die 'retirement unlink failed' if -e $path || -l $path; },
        rmdir=>sub { rmdir($_[0]) or die 'retirement directory failed'; }
    };
    $ops->{receipt}->($owner);
    for my $name ('input','control','terminal') {
        my $path="$owner->{path}/$name";
        $ops->{unlink}->($path);
    }
    $ops->{rmdir}->($owner->{path});
}
sub finish_owned {
    my ($anchor, $established, $ops) = @_;
    $ops //= {
        signal => sub { my ($group,$sig,$pid) = @_; my $n = kill($sig, $group ? -$pid : $pid); die "owned signal refused: $!" if !$n && $! != ESRCH; },
        pause => sub { sleep(.15); },
        reap => sub { my ($pid) = @_; my $n; do { $n = waitpid($pid,0); } while $n < 0 && $!{EINTR}; die 'owned anchor reap failed' unless $n == $pid; }
    };
    my $error;
    eval {
        if ($established) {
            $ops->{signal}->(1,'TERM',$anchor);
            $ops->{pause}->();
            $ops->{signal}->(1,'KILL',$anchor);
        } else { $ops->{signal}->(0,'KILL',$anchor); }
    }; $error = $@;
    if ($error) { $ops->{signal}->(0,'KILL',$anchor); }
    $ops->{reap}->($anchor);
    die $error if $error;
}
sub forward_add {
    my ($pending,$index,$bytes) = @_;
    my $room = $MAX-length($pending->[$index]);
    $pending->[$index] .= substr($bytes,0,$room);
    return length($bytes) <= $room;
}
sub forward_flush {
    my ($pending,$writer) = @_;
    $writer //= sub { my ($index,$data)=@_; my $fd=$index==0 ? \*STDOUT : \*STDERR; return syswrite($fd,$data); };
    for my $index (0,1) {
        next unless length($pending->[$index]);
        my $n = $writer->($index,$pending->[$index]);
        if (defined($n) && $n > 0) { substr($pending->[$index],0,$n,''); }
        elsif (!defined($n) && ($! == EAGAIN || $! == EWOULDBLOCK)) { next; }
        else { return 0; }
    }
    return 1;
}
sub run {
    my ($req, $controller, $stream) = @_;
    die 'invalid request nonce' unless $req->{nonce} =~ /\A[0-9a-f]{1,96}\z/;
    my $duration = $req->{milliseconds}/1000;
    die 'invalid request deadline' unless $duration > 0 && $duration <= 7*86400;
    my $o = identity({v=>1, nonce=>$req->{nonce}, path=>'/tmp/gitcraft-owned-'.random_hex(16), token=>random_hex(32)});
    mkdir($o->{path}, 0700) or die 'private owner directory failed';
    my $server = IO::Socket::UNIX->new(Type=>SOCK_STREAM, Local=>"$o->{path}/control", Listen=>4) or die 'private control socket failed';
    $server->blocking(0); cloexec($server);
    sysopen(my $input, "$o->{path}/input", O_RDWR|O_CREAT|O_EXCL|O_NOFOLLOW, 0600) or die 'input file failed';
    die 'invalid encoded input' unless $req->{input} =~ /\A(?:[A-Za-z0-9+\/]{4})*(?:[A-Za-z0-9+\/]{2}==|[A-Za-z0-9+\/]{3}=)?\z/;
    binmode($input); print $input decode_base64($req->{input}) or die 'input write failed';
    seek($input, 0, 0) or die 'input seek failed';
    pipe(my $out_r, my $out_w) or die 'pipe failed';
    pipe(my $err_r, my $err_w) or die 'pipe failed';
    pipe(my $status_r, my $status_w) or die 'pipe failed';
    pipe(my $gate_r, my $gate_w) or die 'pipe failed';
    cloexec($_) for ($input,$out_r,$out_w,$err_r,$err_w,$status_r,$status_w,$gate_r,$gate_w);
    my $anchor = fork(); die 'anchor fork failed' unless defined($anchor);
    if (!$anchor) {
        eval {
            close($server); close($controller); close($out_r); close($err_r); close($status_r); close($gate_w);
            setsid() >= 0 or die 'anchor setsid failed';
            $SIG{TERM} = 'IGNORE';
            syswrite($status_w, "ready\n") == 6 or die 'anchor ready failed';
            my $gate; sysread($gate_r, $gate, 1) == 1 && $gate eq 'G' or _exit(125);
            close($gate_r);
            my $child = fork(); die 'command fork failed' unless defined($child);
            if (!$child) {
                $SIG{TERM} = 'DEFAULT'; $SIG{HUP} = 'DEFAULT'; $SIG{PIPE} = 'DEFAULT';
                open(STDIN, '<&', $input) or _exit(125); open(STDOUT, '>&', $out_w) or _exit(125); open(STDERR, '>&', $err_w) or _exit(125);
                close($input); close($status_w); close($out_w); close($err_w);
                exec('/bin/sh', '-c', $req->{script}) or _exit(125);
            }
            close($out_w); close($err_w); close($input);
            my $waited; do { $waited = waitpid($child, 0); } while $waited < 0 && $!{EINTR};
            die 'command reap failed' unless $waited == $child;
            my $raw = $?; my $code = ($raw & 127) ? 128 + ($raw & 127) : $raw >> 8;
            syswrite($status_w, "$code\n"); close($status_w);
            while (1) { sleep(86400); }
        };
        _exit(125);
    }
    close($out_w); close($err_w); close($status_w); close($gate_r); close($input);
    my @pending = ('','');
    my $code = 125; my @output = ('',''); my %clients; my $established = 0;
    local $SIG{HUP} = 'IGNORE';
    my $error;
    eval {
        my $ready = IO::Select->new($status_r);
        $ready->can_read(5) or die 'anchor readiness timed out';
        my $answer; sysread($status_r, $answer, 6) == 6 && $answer eq "ready\n" or die 'anchor readiness failed';
        $established = 1;
        atomic($req->{record}, encode($o), 1) if $req->{record};
        my $hello = "O1 $o->{nonce} $o->{path} $o->{token}\n";
        if ($stream) { print STDERR "__GITCRAFT_OWNER__ $hello"; STDERR->flush; }
        else { print STDOUT $hello; STDOUT->flush; }
        if ($stream) { STDOUT->blocking(0); STDERR->blocking(0); }
        syswrite($gate_w, 'G') == 1 or die 'anchor release failed'; close($gate_w); undef($gate_w);
        my $select = IO::Select->new($controller,$server,$status_r,$out_r,$err_r);
        my %kind = (fileno($controller)=>'controller',fileno($server)=>'socket',fileno($status_r)=>'status',fileno($out_r)=>0,fileno($err_r)=>1);
        my $deadline = time()+$duration; my $status = ''; my $stopped = 0;
        while (!$stopped && time() < $deadline) {
            last if $stream && !forward_flush(\@pending);
            for my $key (keys %clients) { if (time() > $clients{$key}{until}) { $select->remove($clients{$key}{handle}); close($clients{$key}{handle}); delete $clients{$key}; } }
            for my $fd ($select->can_read(.05)) {
                my $which = $kind{fileno($fd)} // 'client';
                if ($which eq 'controller') { my $n = sysread($controller, my $bytes, 4096); $stopped = 1 if !defined($n) || !$n; }
                elsif ($which eq 'socket') {
                    my $client = $server->accept(); next unless $client;
                    if (keys(%clients) >= 4) { close($client); next; }
                    $client->blocking(0); cloexec($client);
                    $clients{fileno($client)} = {handle=>$client,bytes=>'',until=>time()+.25}; $select->add($client);
                }
                elsif ($which eq 'client') {
                    my $client = $clients{fileno($fd)};
                    my $n = sysread($fd, my $bytes, 2049-length($client->{bytes})); next if !defined($n) && ($! == EAGAIN || $! == EWOULDBLOCK);
                    $client->{bytes} .= $bytes // '';
                    if (!$n || length($client->{bytes}) > 2048 || $client->{bytes} =~ /\n/) {
                        $select->remove($fd);
                        my $valid = eval { encode($JSON->decode($client->{bytes})) eq encode({%$o,op=>'stop'}) };
                        if ($valid) { $stopped = 1; $client->{until} = time()+10; }
                        else { delete $clients{fileno($fd)}; close($fd); }
                    }
                }
                elsif ($which eq 'status') {
                    my $n = sysread($status_r, my $bytes, 64); $status .= $bytes // '';
                    if ($status =~ /\n/) { $status =~ /\A([0-9]{1,3})\n\z/ or die 'invalid command status'; $code = 0+$1; $stopped = 1; }
                    elsif (!$n) { die 'anchor lost before command status'; }
                }
                else {
                    my $n = sysread($fd, my $bytes, 8192);
                    if (!$n) { $select->remove($fd); next; }
                    my $room = $MAX-length($output[$which]); $stopped = 1 if length($bytes) > $room;
                    $output[$which] .= substr($bytes,0,$room);
                    $stopped = 1 if $stream && !forward_add(\@pending,$which,$bytes);
                }
            }
        }
    }; $error = $@;
    # Always finish the same unreaped anchor, even when the controller/output is lost.
    finish_owned($anchor,$established);
    close($gate_w) if $gate_w;
    my $end = time()+.3;
    for my $index (0,1) {
        my $fd = $index == 0 ? $out_r : $err_r;
        my $select = IO::Select->new($fd);
        while (time() < $end && $select->can_read(0)) {
            my $n = sysread($fd,my $bytes,8192); last unless $n;
            $output[$index] .= substr($bytes,0,$MAX-length($output[$index]));
            forward_add(\@pending,$index,$bytes) if $stream;
        }
        close($fd);
    }
    while ($stream && time() < $end && (length($pending[0]) || length($pending[1]))) { last unless forward_flush(\@pending); sleep(.005); }
    close($status_r); close($server);
    my $final = {%$o,state=>'stopped',code=>$code}; atomic("$o->{path}/terminal",encode($final),0);
    for my $client (values %clients) { syswrite($client->{handle},encode($final)); close($client->{handle}); }
    die $error if $error;
    return ($code,\@output,$o);
}
sub serve {
    open(my $controller, '<&', \*STDIN) or die 'controller missing'; cloexec($controller);
    STDOUT->autoflush(1); binmode(STDOUT); binmode(STDERR); binmode($controller);
    print "__GITCRAFT_READY__\n";
    my $previous;
    while (1) {
        my $line = '';
        while (length($line) <= (96 << 20)) { my $n = sysread($controller,my $part,65536); return unless $n; $line .= $part; last if index($part,"\n") >= 0; }
        die 'pipelined request rejected' unless $line =~ /\A[^\n]*\n\z/s;
        die 'oversized request' if length($line) > (96 << 20);
        if ($line =~ /\AA ([0-9a-f]{1,96})\n\z/) {
            die 'invalid retirement acknowledgement' unless $previous && $previous->{nonce} eq $1;
            retire($previous); undef($previous); next;
        }
        if ($line =~ /\AP ([0-9a-f]{1,96})\n\z/) { print $line; next; }
        my $req = $JSON->decode($line); print "S $req->{nonce}\n";
        my ($code,$out,$owner) = run($req,$controller,0);
        $previous=$owner;
        print "C1 ",encode({%$owner,state=>'stopped',code=>$code});
        print "R $req->{nonce} $code ",length($out->[0]),' ',length($out->[1]),"\n",$out->[0],$out->[1];
    }
}
my $mode = shift(@ARGV) // '';
if ($mode eq 'serve' || $mode eq 'stream') {
    my $supervisor = fork(); die 'supervisor fork failed' unless defined($supervisor);
    if ($supervisor) { waitpid($supervisor,0); my $status=$?; exit(($status & 127) ? 128+($status & 127) : $status >> 8); }
    setsid() >= 0 or die 'supervisor setsid failed'; $SIG{HUP}='IGNORE';
}
if ($mode eq 'probe') { exit(0); }
elsif ($mode eq 'serve') { serve(); }
elsif ($mode eq 'retire') { my $line = <STDIN>; retire($JSON->decode($line)); print "retired\n"; }
elsif ($mode eq 'control') { my $line = <STDIN>; print encode(control($JSON->decode($line))); }
elsif ($mode eq 'stream') {
    print STDOUT "\n__GITCRAFT_READY__\n"; STDOUT->flush;
    my $path = shift(@ARGV); sysopen(my $f,$path,O_RDONLY|O_NOFOLLOW) or die 'request missing'; local $/; my $req = $JSON->decode(<$f>); close($f); unlink($path); (my $dir = $path) =~ s{/[^/]+$}{}; rmdir($dir);
    open(my $controller,'<&',\*STDIN) or die 'controller missing'; cloexec($controller);
    my ($code,$out) = run($req,$controller,1); exit($code);
}
elsif ($mode eq 'recording') {
    my @retired;
    my $retiring={v=>1,nonce=>'a',path=>'/tmp/gitcraft-owned-'.('b'x32),token=>'c'x64};
    my $retire_ops={receipt=>sub {die 'missing atomic receipt';},unlink=>sub {push @retired,$_[0];},rmdir=>sub {push @retired,$_[0];}};
    my $erased=eval {retire($retiring,$retire_ops);1;};
    die 'retired without proof' if $erased || @retired;
    $retire_ops->{receipt}=sub {};
    retire($retiring,$retire_ops);
    die 'unexpected retirement target' unless join(',',@retired) eq join(',',map {"$retiring->{path}/$_"} ('input','control','terminal')).",$retiring->{path}";
    my @queued=('abcdef','');
    die 'blocked forward lost bytes' unless forward_flush(\@queued,sub {$!=EAGAIN; return undef;}) && $queued[0] eq 'abcdef';
    die 'partial forward lost bytes' unless forward_flush(\@queued,sub {return 2;}) && $queued[0] eq 'cdef';
    {
        my $previous=$MAX; $MAX=8;
        die 'unbounded forward queue' if forward_add(\@queued,0,'x'x16) || length($queued[0]) != 8;
        $MAX=$previous;
    }
    for my $established (0,1) {
        my @events;
        my $ops = {signal=>sub {push @events, join(':',@_);}, pause=>sub {}, reap=>sub {push @events, "reap:$_[0]";}};
        finish_owned(424242,$established,$ops);
        my $expected = $established ? '1:TERM:424242,1:KILL:424242,reap:424242' : '0:KILL:424242,reap:424242';
        die 'recording order failed' unless join(',',@events) eq $expected;
    }
    my @events;
    my $ops = {signal=>sub {push @events, $_[0] ? 'group' : 'direct'; die 'refused' if $_[0];},pause=>sub {},reap=>sub {push @events,'reap';}};
    my $ok = eval {finish_owned(424242,1,$ops);1;};
    die 'refusal acknowledged' if $ok || join(',',@events) ne 'group,direct,reap';
    print "8 recording-only owner gates passed\n";
}
else { die 'unknown owner operation'; }

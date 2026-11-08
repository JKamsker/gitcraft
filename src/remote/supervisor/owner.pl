# gitcraft owned execution protocol v1. Perl POSIX/core-module fallback.
use strict;
use warnings;
use POSIX qw(setsid _exit);
use IO::Socket::UNIX;
use IO::Select;
use JSON::PP;
use MIME::Base64 qw(decode_base64 encode_base64);
use Fcntl qw(O_RDONLY O_WRONLY O_RDWR O_CREAT O_EXCL O_NOFOLLOW O_DIRECTORY O_NONBLOCK S_ISDIR S_ISREG F_GETFD F_SETFD FD_CLOEXEC LOCK_EX LOCK_NB LOCK_UN);
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
    my ($o,$relative) = @_;
    my $f;
    if (!sysopen($f, ($relative // "$o->{path}/terminal"), O_RDONLY|O_NOFOLLOW|O_NONBLOCK)) {
        return undef if $!{ENOENT};
        die 'receipt open failed';
    }
    my @info=stat($f);
    die 'receipt is not private regular file' unless @info && S_ISREG($info[2]) && $info[4] == $> && !($info[2]&0077) && $info[7] <= 2048;
    my $bytes = ''; my $n=read($f, $bytes, 2049); close($f);
    die 'receipt read failed' unless defined($n);
    die 'oversized receipt' if length($bytes) > 2048;
    return checked_terminal($o,$bytes);
}
sub checked_terminal {
    my ($o,$bytes)=@_;die 'oversized terminal proof' if length($bytes)>2048;
    my $v = $JSON->decode($bytes);
    die 'invalid receipt' unless ref($v) eq 'HASH' && join(',',sort keys %$v) eq 'code,complete,nonce,path,state,token,v';
    for my $k (keys %$o) { die 'unconfirmed receipt' unless $v->{$k} eq $o->{$k}; }
    die 'unconfirmed receipt' unless $v->{state} eq 'stopped' && $v->{code} =~ /\A[0-9]+\z/ && $v->{code} <= 255 && JSON::PP::is_bool($v->{complete});
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
sub pinned_directory {
    my ($path,$body,$missing)=@_;my $dir;
    if (!sysopen($dir,$path,O_RDONLY|O_DIRECTORY|O_NOFOLLOW)) {return 0 if $missing && $!{ENOENT};die 'endpoint directory unavailable';}
    my @info=stat($dir);
    die 'endpoint directory is not private' unless @info && S_ISDIR($info[2]) && $info[4] == $> && ($info[2]&0777)==0700;
    sysopen(my $saved,'.',O_RDONLY|O_DIRECTORY|O_NOFOLLOW) or die 'cannot retain current directory';
    # Core Perl chdir(FILEHANDLE) uses fchdir; unsupported platforms fail closed.
    chdir($dir) or die 'cannot pin endpoint directory';
    my $ok=eval {$body->();1;}; my $error=$@;
    chdir($saved) or die 'cannot restore current directory';
    close($saved); close($dir);
    die $error unless $ok;return 1;
}
sub retire {
    my ($owner,$ops)=@_; identity($owner);
    $ops //= {
        pin=>\&pinned_directory,
        receipt=>sub { terminal($_[0],'terminal') or die 'missing atomic retirement receipt'; },
        unlink=>sub { my $ok=unlink($_[0]); die 'retirement unlink failed' if !$ok && !$!{ENOENT}; }
    };
    $ops->{pin}->($owner->{path},sub {
        $ops->{receipt}->($owner);
        for my $name ('input','control','terminal') { $ops->{unlink}->($name); }
    });
    # Retain the empty directory; do not rmdir a possibly substituted pathname.
}
# Stable external claim/fence objects; never unlink/recreate them during cleanup.
sub workspace_now {Time::HiRes::clock_gettime(Time::HiRes::CLOCK_MONOTONIC())}
sub workspace_id {my ($f)=@_;my @s=stat($f);die 'workspace identity unavailable' unless @s;return sprintf('unix:%016x:%016x',$s[0],$s[1]);}
sub workspace_namespace {my ($want,$have)=@_;die 'workspace namespace replaced' unless @$want==4 && join("\n",@$want) eq join("\n",@$have);}
sub workspace_grant {my ($a,$g,$actual_a,$actual_g)=@_;die 'stale workspace generation; no mutation authorized' unless $a eq $actual_a && "$g\n" eq $actual_g;}
sub workspace_open {
    my ($name,$flags,$optional)=@_;my $f;
    if (!sysopen($f,$name,$flags|O_NOFOLLOW|O_NONBLOCK,0600)) {return undef if $optional && $!{ENOENT};die "workspace open failed: $!";}
    cloexec($f);my @s=stat($f);
    die 'workspace file is not private single-link regular file' unless @s && S_ISREG($s[2]) && $s[4]==$> && !($s[2]&0077) && $s[3]==1;
    return $f;
}
sub workspace_read {
    my ($f,$limit)=@_;seek($f,0,0) or die 'workspace seek failed';my $n=read($f,my $data,$limit+1);die 'workspace read failed' unless defined($n);die 'oversized workspace file' if length($data)>$limit;return $data;
}
sub workspace_relative {
    my ($w,$body)=@_;
    sysopen(my $saved,'.',O_RDONLY|O_DIRECTORY|O_NOFOLLOW) or die 'cannot retain workspace cwd';
    chdir($w->{dir}) or die 'cannot pin workspace directory';
    my ($result,$ok,$error);$ok=eval {$result=$body->();1;};$error=$@;
    chdir($saved) or die 'cannot restore workspace cwd';close($saved);die $error unless $ok;return $result;
}
sub workspace_directory {
    my ($root)=@_;sysopen(my $dir,$root,O_RDONLY|O_DIRECTORY|O_NOFOLLOW) or die 'workspace directory unavailable';cloexec($dir);
    my @s=stat($dir);die 'workspace directory is not private' unless @s && S_ISDIR($s[2]) && $s[4]==$> && ($s[2]&0777)==0700;return $dir;
}
sub workspace_verify {
    my ($w)=@_;my $named=workspace_directory($w->{root});my @ids=(workspace_id($named));close($named);
    workspace_relative($w,sub {for my $name ('primary','fence','generation') {my $f=workspace_open($name,O_RDONLY);push @ids,workspace_id($f);close($f);}});
    workspace_namespace($w->{ids},\@ids);
    my $anchor=workspace_open("$w->{root}.anchor",O_RDONLY);
    die 'workspace anchor replaced or incomplete' unless workspace_id($anchor) eq $w->{anchor_id} && workspace_read($anchor,4096) eq $w->{data} && workspace_read($w->{anchor},4096) eq $w->{data};close($anchor);
}
sub workspace_close {
    my ($w)=@_;return unless $w;
    # Close only. Explicit LOCK_UN in the forked anchor would unlock the parent too.
    close($_) for @{$w->{files} // []};$w->{files}=[];
    for my $key ('anchor','dir') {close($w->{$key}) if defined($w->{$key});delete $w->{$key};}
}
sub workspace_existing {
    my ($root,$initialize)=@_;die 'workspace root must be absolute' unless defined($root) && $root =~ m{\A/};
    my $w={root=>$root,files=>[]};my $ok=eval {
        my $anchor;
        if (!sysopen($anchor,"$root.anchor",O_RDONLY|O_NOFOLLOW|O_NONBLOCK)) {
            die 'workspace anchor unavailable' unless $!{ENOENT} && $initialize;
            mkdir($root,0700) or die 'workspace namespace already exists or cannot initialize';
            $w->{dir}=workspace_directory($root);
            workspace_relative($w,sub {$w->{files}=[map {workspace_open($_,O_RDWR|O_CREAT|O_EXCL)} ('primary','fence','generation')];});
            flock($w->{files}[0],LOCK_EX|LOCK_NB) or die 'workspace initialization claim refused';
            $w->{ids}=[workspace_id($w->{dir}),map {workspace_id($_)} @{$w->{files}}];
            $w->{data}="gitcraft-workspace-anchor-v1\n".join("\n",@{$w->{ids}})."\n";
            atomic("$root.anchor",$w->{data},1);$w->{anchor}=workspace_open("$root.anchor",O_RDONLY);$w->{initialized}=1;
        } else {
            close($anchor);$w->{anchor}=workspace_open("$root.anchor",O_RDONLY);
            $w->{dir}=workspace_directory($root);
            workspace_relative($w,sub {$w->{files}=[map {workspace_open($_,O_RDWR)} ('primary','fence','generation')];});
            $w->{ids}=[workspace_id($w->{dir}),map {workspace_id($_)} @{$w->{files}}];
            $w->{data}="gitcraft-workspace-anchor-v1\n".join("\n",@{$w->{ids}})."\n";
        }
        $w->{anchor_id}=workspace_id($w->{anchor});workspace_verify($w);1;
    };my $error=$@;if (!$ok) {workspace_close($w);die $error;}return $w;
}
sub workspace_wait {
    my ($controller,$deadline)=@_;die "workspace lease expired\n" if workspace_now()>=$deadline;
    if (IO::Select->new($controller)->can_read(0)) {my $n=sysread($controller,my $byte,4096);die "workspace controller lost\n" if !defined($n) || !$n;die 'unexpected workspace controller input';}
}
sub workspace_lock_fence {
    my ($w,$controller,$deadline)=@_;
    while (1) {workspace_wait($controller,$deadline);last if flock($w->{files}[1],LOCK_EX|LOCK_NB);die 'workspace fence locking unsupported' unless $!{EAGAIN} || $!{EWOULDBLOCK};sleep(.005);}
    workspace_verify($w);
}
sub workspace_claim {
    my ($request,$controller,$deadline)=@_;die 'invalid workspace generation' unless ($request->{generation}//'') =~ /\A[0-9a-f]{64}\z/;
    my $w=workspace_existing($request->{root},1);my $ok=eval {
        flock($w->{files}[0],LOCK_EX|LOCK_NB) or die 'another workspace claimant owns repository' unless $w->{initialized};
        workspace_lock_fence($w,$controller,$deadline);workspace_wait($controller,$deadline);
        my $g=$w->{files}[2];seek($g,0,0) or die 'generation seek failed';truncate($g,0) or die 'generation truncate failed';print $g "$request->{generation}\n" or die 'generation write failed';$g->flush && $g->sync or die 'generation sync failed';
        flock($w->{files}[1],LOCK_UN) or die 'fence release failed';1;
    };my $error=$@;if (!$ok) {workspace_close($w);die $error;}return $w;
}
sub workspace_fence {
    my ($grant,$controller,$deadline)=@_;
    die 'invalid workspace grant' unless ref($grant) eq 'HASH' && join(',',sort keys %$grant) eq 'anchor,generation,mode,root' && ($grant->{generation}//'') =~ /\A[0-9a-f]{64}\z/ && ($grant->{mode}//'') =~ /\A(?:dispatch|mutation)\z/;
    my $w=workspace_existing($grant->{root});my $ok=eval {
        workspace_lock_fence($w,$controller,$deadline);
        workspace_grant($grant->{anchor},$grant->{generation},$w->{anchor_id},workspace_read($w->{files}[2],128));
        if (flock($w->{files}[0],LOCK_EX|LOCK_NB)) {die 'workspace primary owner no longer live';}
        die 'workspace primary ownership unknown' unless $!{EAGAIN} || $!{EWOULDBLOCK};
        $w->{grant}=$grant;1;
    };my $error=$@;if (!$ok) {workspace_close($w);die $error;}return $w;
}
sub workspace_publish {
    my ($w,$path,$owner)=@_;my $g=$w->{grant};my $prefix="$w->{root}/";
    die 'owner record outside workspace ledger' unless index($path,$prefix)==0;
    my $name=substr($path,length($prefix));die 'owner record generation/kind mismatch' unless $name =~ /\A\Q$g->{generation}-$g->{mode}-\E[0-9a-f]{32}\.owner\z/;
    workspace_verify($w);my $data=encode({v=>3,anchor=>$g->{anchor},generation=>$g->{generation},kind=>$g->{mode},endpoint=>$owner});die 'oversized workspace owner' if length($data)>2048;
    workspace_relative($w,sub {atomic($name,$data,1);});
}
sub claim_workspace {
    my ($request,$controller)=@_;my $duration=($request->{milliseconds}//0)/1000;die 'invalid workspace lifetime' unless $duration>0 && $duration<=7*86400;
    my $deadline=workspace_now()+$duration;my $w=workspace_claim($request,$controller,$deadline);
    my $ok=eval {emit(\*STDOUT,['__GITCRAFT_WORKSPACE_CLAIM__ '.encode({root=>$request->{root},anchor=>$w->{anchor_id},generation=>$request->{generation}})]);while (1) {workspace_wait($controller,$deadline);sleep(.01);}1;};my $error=$@;workspace_close($w);
    die $error unless $ok || $error eq "workspace controller lost\n" || $error eq "workspace lease expired\n";
}

# External ledger format matches native/Python. All disk evidence is bounded and private.
use Digest::SHA qw(sha256_hex);
my $LEDGER_MAX_OWNERS=4096;my $LEDGER_MAX_PROOF=9<<10;my $LEDGER_MAX_MANIFEST=40<<20;my $LEDGER_MAX_EVIDENCE=64<<20;
sub ledger_binding {
    my ($name,$bytes,$anchor)=@_;die 'oversized workspace owner' if length($bytes)>2048;
    my $v=$JSON->decode($bytes);die 'invalid bound workspace record' unless ref($v) eq 'HASH' && join(',',sort keys %$v) eq 'anchor,endpoint,generation,kind,v' && $v->{v}==3 && $v->{anchor} eq $anchor && $v->{generation}=~/\A[0-9a-f]{64}\z/ && $v->{kind}=~/\A(?:dispatch|mutation)\z/;
    die 'record generation/name mismatch' unless $name =~ /\A\Q$v->{generation}-$v->{kind}-\E[0-9a-f]{32}\.owner\z/;
    return identity($v->{endpoint});
}
sub ledger_decode {
    my ($p,$anchor)=@_;die 'invalid bounded proof' unless ref($p) eq 'HASH' && join(',',sort keys %$p) eq 'name,receipt,record' && length($p->{record})<=2732 && length($p->{receipt})<=5464;
    for my $k ('record','receipt') {die 'invalid proof base64' unless $p->{$k}=~/\A(?:[A-Za-z0-9+\/]{4})*(?:[A-Za-z0-9+\/]{2}==|[A-Za-z0-9+\/]{3}=)?\z/;}
    my $record=decode_base64($p->{record});my $receipt=decode_base64($p->{receipt});my $owner=ledger_binding($p->{name},$record,$anchor);checked_terminal($owner,$receipt);
    return {name=>$p->{name},record=>$record,receipt=>$receipt,owner=>$owner,present=>0};
}
sub ledger_proof {my ($e)=@_;return {name=>$e->{name},record=>encode_base64($e->{record},''),receipt=>encode_base64($e->{receipt},'')};}
sub ledger_merge {
    my ($entries,$e)=@_;my $old=$entries->{$e->{name}};
    if ($old) {die 'changed ledger evidence' unless $old->{record} eq $e->{record} && $old->{receipt} eq $e->{receipt};}
    else {$entries->{$e->{name}}=$e;}
    die 'workspace exceeds 4096 recovered owners' if keys(%$entries)>$LEDGER_MAX_OWNERS;
}
sub ledger_raw_read {
    my ($p,$name,$limit,$optional)=@_;
    return workspace_relative($p->{locks},sub {my $f=workspace_open($name,O_RDONLY,$optional);return undef unless $f;my $data=workspace_read($f,$limit);close($f) or die 'ledger close failed';return $data;});
}
sub ledger_active {my ($p)=@_;workspace_wait($p->{controller},$p->{deadline});}
sub ledger_read {
    my ($p,$name,$limit)=@_;ledger_active($p);$limit=$p->{budget} if $p->{budget}<$limit;my $data=ledger_raw_read($p,$name,$limit);$p->{budget}-=length($data);return $data;
}
sub ledger_publish {
    my ($p,$name,$bytes,$limit)=@_;die 'ledger publication bound' if length($bytes)>$limit;
    workspace_verify($p->{locks});workspace_relative($p->{locks},sub {atomic($name,$bytes,1);});
}
sub ledger_verify {
    my ($p)=@_;workspace_verify($p->{locks});
    for my $e (values %{$p->{entries}}) {my $current=ledger_raw_read($p,$e->{name},2048,1);die 'owner set changed during recovery' if ($e->{present} && (!defined($current) || $current ne $e->{record})) || (!$e->{present} && defined($current));}
}
sub ledger_remove {
    my ($p,$name,$expected,$limit)=@_;die 'ledger entry changed before pruning' unless ledger_raw_read($p,$name,$limit) eq $expected;
    workspace_relative($p->{locks},sub {unlink($name) or die 'ledger removal failed';});
}
sub ledger_retire_proven {
    my ($owner,$proof)=@_;checked_terminal($owner,$proof);
    retire($owner,{pin=>sub {pinned_directory($_[0],$_[1],1);},receipt=>sub {checked_terminal($owner,$proof);},unlink=>sub {my $ok=unlink($_[0]);die 'proven retirement unlink failed' if !$ok && !$!{ENOENT};}});
}
sub ledger_prepare {
    my ($locks,$record,$controller,$deadline)=@_;(my $own=$record)=~s{.*/}{};
    my $p={locks=>$locks,grant=>$locks->{grant},controller=>$controller,deadline=>$deadline,own=>$own,entries=>{},manifests=>[],budget=>$LEDGER_MAX_EVIDENCE};
    opendir(my $dir,$locks->{root}) or die 'ledger inventory unavailable';my %names;
    while (defined(my $name=readdir($dir))) {next if $name eq '.' || $name eq '..';$names{$name}=1;die 'workspace inventory bound; preserved' if keys(%names)>$LEDGER_MAX_OWNERS*3+3;}
    closedir($dir) or die 'ledger inventory close failed';
    for my $name (keys %names) {die 'unknown external ledger evidence' unless $name=~/\A(?:primary|fence|generation)\z/ || $name=~/\.owner(?:\.proof|\.cleaned)?\z/;}
    die 'cleanup owner not indexed' unless $names{$own};
    for my $name (sort grep {/\.cleaned\z/} keys %names) {
        my $data=ledger_read($p,$name,$LEDGER_MAX_MANIFEST);my $m=$JSON->decode($data);
        die 'invalid cleaned manifest' unless ref($m) eq 'HASH' && join(',',sort keys %$m) eq 'anchor,entries,generation,owner,v' && $m->{v}==1 && $m->{anchor} eq $p->{grant}{anchor} && "$m->{owner}.cleaned" eq $name && $m->{generation}=~/\A[0-9a-f]{64}\z/ && $m->{owner}=~/\A\Q$m->{generation}-mutation-\E[0-9a-f]{32}\.owner\z/ && ref($m->{entries}) eq 'ARRAY' && @{$m->{entries}}<=$LEDGER_MAX_OWNERS;
        ledger_merge($p->{entries},ledger_decode($_,$p->{grant}{anchor})) for @{$m->{entries}};push @{$p->{manifests}},[$name,$data];
    }
    for my $name (sort grep {/\.owner\z/ && $_ ne $own} keys %names) {
        my $record=ledger_read($p,$name,2048);my $owner=ledger_binding($name,$record,$p->{grant}{anchor});my $old=$p->{entries}{$name};
        if ($old) {die 'owner changed after cleaned proof' unless $old->{record} eq $record;$old->{present}=1;}
        else {$p->{entries}{$name}={name=>$name,record=>$record,receipt=>'',owner=>$owner,present=>1};}
    }
    die 'workspace exceeds 4096 recovered owners' if keys(%{$p->{entries}})>$LEDGER_MAX_OWNERS;
    for my $name (sort grep {/\.proof\z/} keys %names) {
        my $proof=$JSON->decode(ledger_read($p,$name,$LEDGER_MAX_PROOF));die 'proof name mismatch' unless "$proof->{name}.proof" eq $name;
        my $e=ledger_decode($proof,$p->{grant}{anchor});my $old=$p->{entries}{$e->{name}};
        die 'orphan or changed proof without cleaned manifest' unless $old && $old->{record} eq $e->{record} && (!$old->{receipt} || $old->{receipt} eq $e->{receipt});$old->{receipt}=$e->{receipt};
    }
    ledger_verify($p);
    for my $e (values %{$p->{entries}}) {ledger_active($p);if (!length($e->{receipt})) {$e->{receipt}=encode(control($e->{owner}));ledger_publish($p,"$e->{name}.proof",encode(ledger_proof($e)),$LEDGER_MAX_PROOF);}}
    ledger_verify($p);return $p;
}
sub ledger_shell_proof {
    my ($p)=@_;my %generations=($p->{grant}{generation}=>1);$generations{substr($_,0,64)}=1 for keys %{$p->{entries}};
    my @generations=sort keys %generations;my @records=grep {$p->{entries}{$_}{present}} sort keys %{$p->{entries}};
    my @lines=('G '.@generations,@generations,'R '.@records);for my $name (@records) {push @lines,$name,unpack('H*',$p->{entries}{$name}{record});}return join("\n",@lines)."\n";
}
sub ledger_complete {
    my ($p)=@_;ledger_active($p);ledger_verify($p);
    my $data=encode({v=>1,anchor=>$p->{grant}{anchor},generation=>$p->{grant}{generation},owner=>$p->{own},entries=>[map {ledger_proof($p->{entries}{$_})} sort keys %{$p->{entries}}]});
    my $manifest="$p->{own}.cleaned";ledger_publish($p,$manifest,$data,$LEDGER_MAX_MANIFEST);
    for my $e (values %{$p->{entries}}) {ledger_active($p);ledger_retire_proven($e->{owner},$e->{receipt});}
    for my $e (values %{$p->{entries}}) {
        ledger_active($p);ledger_remove($p,$e->{name},$e->{record},2048) if $e->{present};my $name="$e->{name}.proof";
        my $cache=ledger_raw_read($p,$name,$LEDGER_MAX_PROOF,1);next unless defined($cache);my $proof=ledger_decode($JSON->decode($cache),$p->{grant}{anchor});
        die 'proof changed before pruning' unless $proof->{name} eq $e->{name} && $proof->{record} eq $e->{record} && $proof->{receipt} eq $e->{receipt};ledger_remove($p,$name,$cache,$LEDGER_MAX_PROOF);
    }
    ledger_remove($p,$_->[0],$_->[1],$LEDGER_MAX_MANIFEST) for @{$p->{manifests}};ledger_remove($p,$manifest,$data,$LEDGER_MAX_MANIFEST);
    my $digest=Digest::SHA->new(256);for my $name (sort keys %{$p->{entries}}) {$digest->add($name,"\0",$p->{entries}{$name}{record},"\0");}
    return encode({v=>1,state=>'cleaned',anchor=>$p->{grant}{anchor},generation=>$p->{grant}{generation},records=>scalar(keys %{$p->{entries}}),records_sha256=>$digest->hexdigest});
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
    if ($error) { eval { $ops->{signal}->(0,'KILL',$anchor); }; $error .= $@ if $@; }
    my $reaped=eval { $ops->{reap}->($anchor); 1; }; $error .= $@ unless $reaped;
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
sub emit {
    my ($fd,$chunks,$seconds,$ops)=@_; $seconds //= 5;
    $ops //= {now=>sub {time()},pause=>sub {sleep(.002)},write=>sub {syswrite($fd,$_[0],length($_[0])-$_[1],$_[1])}};
    my $end=$ops->{now}->()+$seconds;
    for my $chunk (@$chunks) {
        my $sent=0;
        while ($sent<length($chunk)) {
            die 'protocol emission deadline' if $ops->{now}->()>=$end;
            my $n=$ops->{write}->($chunk,$sent);
            if (!defined($n)) { if ($!{EAGAIN} || $!{EWOULDBLOCK} || $!{EINTR}) {$ops->{pause}->();next;} die 'protocol emission failed'; }
            die 'protocol emission closed' unless $n>0; $sent+=$n;
        }
    }
    die 'protocol emission deadline' if $ops->{now}->()>=$end;
}
sub handshake_step {
    my ($state,$stopped,$now,$write,$release)=@_;
    return 1 if $state->{released};
    return -1 if $stopped || $now->()>=$state->{until};
    my $n=$write->($state->{pending});
    if (!defined($n)) {return 0 if $!{EAGAIN} || $!{EWOULDBLOCK} || $!{EINTR}; die 'ownership emission failed';}
    die 'ownership emission closed' unless $n>0;
    substr($state->{pending},0,$n,'');
    return 0 if length($state->{pending});
    return -1 if $now->()>=$state->{until};
    $release->(); $state->{released}=1; return 1;
}
sub run {
    my ($req,$controller,$stream)=@_;my $locks;
    die 'cleanup requires full mutation fence' if $req->{cleanup} && (!$req->{workspace} || $req->{workspace}{mode} ne 'mutation');
    if ($req->{workspace}) {
        die 'workspace operations require durable standalone ownership' unless $stream && $req->{record};
        $req->{workspace_deadline}=workspace_now()+$req->{milliseconds}/1000;
        $locks=workspace_fence($req->{workspace},$controller,$req->{workspace_deadline});
    }
    my (@result,$ok,$error);$ok=eval {@result=run_inner($req,$controller,$stream,$locks);1;};$error=$@;
    workspace_close($locks);die $error unless $ok;return @result;
}
sub run_inner {
    my ($req, $controller, $stream,$locks) = @_;
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
            workspace_close($locks);
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
    close($out_w); close($err_w); close($status_w); close($gate_r);
    my @pending = ('',''); my $delivery=1;
    my $code = 125; my @output = ('',''); my %clients; my $established = 0;
    local $SIG{HUP} = 'IGNORE';
    my $error;my $ledger;
    eval {
        my $ready = IO::Select->new($status_r);
        $ready->can_read(5) or die 'anchor readiness timed out';
        my $answer; sysread($status_r, $answer, 6) == 6 && $answer eq "ready\n" or die 'anchor readiness failed';
        $established = 1;
        if ($locks) {workspace_publish($locks,$req->{record},$o);workspace_close($locks) if $req->{workspace}{mode} eq 'dispatch';}
        elsif ($req->{record}) {atomic($req->{record}, encode($o), 1);}
        if ($req->{cleanup}) {
            $ledger=ledger_prepare($locks,$req->{record},$controller,$req->{workspace_deadline});
            seek($input,0,0) && truncate($input,0) or die 'cleanup input reset failed';print $input ledger_shell_proof($ledger) or die 'cleanup proof input failed';$input->flush or die 'cleanup proof flush failed';seek($input,0,0) or die 'cleanup proof rewind failed';
        }
        close($input) or die 'input close failed';
        my $hello = "O1 $o->{nonce} $o->{path} $o->{token}\n";
        STDOUT->blocking(0); STDERR->blocking(0);
        my $handshake={pending=>($stream ? '__GITCRAFT_OWNER__ ' : '').$hello,until=>time()+5};
        my $select = IO::Select->new($controller,$server,$status_r,$out_r,$err_r);
        my %kind = (fileno($controller)=>'controller',fileno($server)=>'socket',fileno($status_r)=>'status',fileno($out_r)=>0,fileno($err_r)=>1);
        my $remaining=defined($req->{workspace_deadline}) ? $req->{workspace_deadline}-workspace_now() : $duration;die 'workspace operation expired before dispatch' unless $remaining>0;
        my $deadline = time()+$remaining; my $status = ''; my $stopped = 0;
        while (!$stopped && time() < $deadline) {
            if ($stream && !forward_flush(\@pending)) { $delivery=0; last; }
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
                    my $room = $MAX-length($output[$which]); if (length($bytes) > $room) { $stopped=1; $delivery=0; }
                    $output[$which] .= substr($bytes,0,$room);
                    if ($stream && ($which!=0 || !$req->{cleanup}) && !forward_add(\@pending,$which,$bytes)) { $stopped=1; $delivery=0; }
                }
            }
            if ($gate_w) {
                my $state=handshake_step($handshake,$stopped,sub {time()},
                    sub {syswrite($stream ? \*STDERR : \*STDOUT,$_[0])},
                    sub {syswrite($gate_w,'G')==1 or die 'anchor release failed';});
                if ($state<0) {$delivery=0;last;}
                if ($state) {close($gate_w);undef($gate_w);}
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
        my $eof=0;
        while (time() < $end) {
            next unless $select->can_read(.002);
            my $n = sysread($fd,my $bytes,8192); if (defined($n) && !$n) { $eof=1; last; } last unless defined($n);
            my $room=$MAX-length($output[$index]); $delivery=0 if length($bytes)>$room;
            $output[$index] .= substr($bytes,0,$room);
            $delivery=0 if $stream && ($index!=0 || !$req->{cleanup}) && !forward_add(\@pending,$index,$bytes);
        }
        $delivery=0 unless $eof;
        close($fd);
    }
    if ($ledger && $code==0 && $delivery) {
        my $ok=eval {my $proof=ledger_complete($ledger);$output[0]=$proof;$delivery=0 if $stream && !forward_add(\@pending,0,$proof);1;};
        if (!$ok) {my $message=substr("cleanup partial; external ledger retained: $@",0,8192);$code=125;$delivery=0;$output[1].=$message;forward_add(\@pending,1,$message) if $stream;}
        $end=time()+.3;
    }
    while ($stream && time() < $end && (length($pending[0]) || length($pending[1]))) { if (!forward_flush(\@pending)) {$delivery=0; last;} sleep(.005); }
    $delivery=0 if $stream && (length($pending[0]) || length($pending[1]));
    close($status_r); close($server);
    my $final = {%$o,state=>'stopped',code=>$code,complete=>$delivery ? JSON::PP::true : JSON::PP::false}; atomic("$o->{path}/terminal",encode($final),0); workspace_close($locks);
    for my $client (values %clients) { eval {emit($client->{handle},[encode($final)],.1);}; close($client->{handle}); }
    die $error if $error;
    return (delivery_code($code,$delivery),\@output,$o);
}
sub delivery_code { my ($code,$complete)=@_; return $complete ? $code : 125; }
sub frame_read {
    my ($state,$receive)=@_;
    while (1) {
        my $end=index($state->{pending},"\n");
        if ($end>=0) {
            die 'oversized request frame' if $end >= (96<<20);
            delete $state->{until};
            return substr($state->{pending},0,$end+1,'');
        }
        die 'oversized request frame' if length($state->{pending}) >= (96<<20);
        $state->{until} //= time()+30 if length($state->{pending});
        die 'incomplete request deadline' if defined($state->{until}) && time()>=$state->{until};
        my $chunk=$receive->(65536);
        if (!defined($chunk) || !length($chunk)) {
            die 'incomplete request frame' if length($state->{pending});
            return undef;
        }
        $state->{pending}.=$chunk;
    }
}
sub serve {
    open(my $controller, '<&', \*STDIN) or die 'controller missing'; cloexec($controller);
    STDOUT->autoflush(1); binmode(STDOUT); binmode(STDERR); binmode($controller);
    STDOUT->blocking(0); STDERR->blocking(0);
    emit(\*STDOUT,["__GITCRAFT_READY__\n"]);
    my $previous;
    my $frames={pending=>''}; my $waiting=IO::Select->new($controller);
    while (1) {
        my $line=frame_read($frames,sub { my ($count)=@_;
            my $left=($frames->{until} // time()+3600)-time();
            die 'controller frame deadline' if $left<=0 || !$waiting->can_read($left);
 my $n=sysread($controller,my $bytes,$count); die 'controller read failed' unless defined($n); return $n ? $bytes : undef; });
        return unless defined($line);
        if ($line =~ /\AA ([0-9a-f]{1,96})\n\z/) {
            die 'invalid retirement acknowledgement' unless $previous && $previous->{nonce} eq $1;
            retire($previous); undef($previous); next;
        }
        if ($line =~ /\AP ([0-9a-f]{1,96})\n\z/) { emit(\*STDOUT,[$line]); next; }
        my $req = $JSON->decode($line); emit(\*STDOUT,["S $req->{nonce}\n"]);
        my ($code,$out,$owner) = run($req,$controller,0);
        $previous=$owner;
        emit(\*STDOUT,['C1 '.encode(terminal($owner)), "R $req->{nonce} $code ".length($out->[0]).' '.length($out->[1])."\n",$out->[0],$out->[1]]);
    }
}
my $mode = shift(@ARGV) // '';
if ($mode eq 'serve' || $mode eq 'stream' || $mode eq 'claim') {
    my $supervisor = fork(); die 'supervisor fork failed' unless defined($supervisor);
    if ($supervisor) { waitpid($supervisor,0); my $status=$?; exit(($status & 127) ? 128+($status & 127) : $status >> 8); }
    setsid() >= 0 or die 'supervisor setsid failed'; $SIG{HUP}='IGNORE';
}
if ($mode eq 'probe') { exit(0); }
elsif ($mode eq 'serve') { serve(); }
elsif ($mode eq 'claim') {
    STDOUT->blocking(0);STDERR->blocking(0);
    open(my $controller,'<&',\*STDIN) or die 'claim controller missing';cloexec($controller);
    my $path=shift(@ARGV);my $f=workspace_open($path,O_RDONLY);my $data=workspace_read($f,4096);close($f);
    unlink($path) or die 'claim request removal failed';(my $dir=$path)=~s{/[^/]+$}{};rmdir($dir) or die 'claim request directory removal failed';
    claim_workspace($JSON->decode($data),$controller);
}
elsif ($mode eq 'retire') { my $line = <STDIN>; retire($JSON->decode($line)); print "retired\n"; }
elsif ($mode eq 'control') { my $line = <STDIN>; print encode(control($JSON->decode($line))); }
elsif ($mode eq 'stream') {
    STDOUT->blocking(0); STDERR->blocking(0);
    emit(\*STDOUT,["\n__GITCRAFT_READY__\n"]);
    my $path = shift(@ARGV); sysopen(my $f,$path,O_RDONLY|O_NOFOLLOW) or die 'request missing'; local $/; my $req = $JSON->decode(<$f>); close($f); unlink($path); (my $dir = $path) =~ s{/[^/]+$}{}; rmdir($dir);
    open(my $controller,'<&',\*STDIN) or die 'controller missing'; cloexec($controller);
    my ($code,$out) = run($req,$controller,1); exit($code);
}
elsif ($mode eq 'recording') {
    {
        my $owner={v=>1,nonce=>'a',path=>'/tmp/gitcraft-owned-'.('b'x32),token=>'c'x64};my $g='d'x64;my $name="$g-mutation-".('e'x32).'.owner';
        my $record=encode({v=>3,anchor=>'original',generation=>$g,kind=>'mutation',endpoint=>$owner});my $receipt=encode({%$owner,state=>'stopped',code=>0,complete=>JSON::PP::true});
        my $proof={name=>$name,record=>encode_base64($record,''),receipt=>encode_base64($receipt,'')};my $entry=ledger_decode($proof,'original');my %entries;ledger_merge(\%entries,$entry);
        die 'changed ledger evidence admitted' if eval {ledger_merge(\%entries,{%$entry,record=>$record.' '});1;};die 'wrong ledger anchor admitted' if eval {ledger_decode($proof,'replacement');1;};
        my @events;my $plan={grant=>{anchor=>'original',generation=>$g},own=>"$g-mutation-".('f'x32).'.owner',entries=>{$name=>{%$entry,present=>1}},manifests=>[]};
        {no warnings 'redefine';local *ledger_active=sub {};local *ledger_verify=sub {push @events,'verify';};local *ledger_publish=sub {push @events,'manifest';};local *ledger_remove=sub {push @events,'prune';};local *ledger_retire_proven=sub {push @events,'retire';die 'recorded refusal';};die 'retirement failure became success' if eval {ledger_complete($plan);1;};}
        die 'retirement failure pruned evidence' unless join(',',@events) eq 'verify,manifest,retire' && $entries{$name}{record} eq $record;
    }
    {
        my @expected=('directory','primary','fence','generation');
        for my $i (0..3) {
            my @changed=@expected;$changed[$i]='replacement';
            die 'replacement granted workspace authority' if eval {workspace_namespace(\@expected,\@changed);1;};
            splice(@changed,$i,1);die 'missing lock granted authority' if eval {workspace_namespace(\@expected,\@changed);1;};
        }
        for my $actual ([original=>('b'x64)."\n"],[replaced=>('a'x64)."\n"],[original=>'partial']) {
            die 'stale generation granted authority' if eval {workspace_grant('original','a'x64,@$actual);1;};
        }
        my @events;
        { no warnings 'redefine';
          local *workspace_existing=sub {push @events,'open';return {anchor_id=>'original',files=>[]};};
          local *workspace_lock_fence=sub {push @events,'lock-and-verify';};
          local *workspace_read=sub {return ('b'x64)."\n";};
          local *workspace_close=sub {push @events,'close-only';};
          die 'queued old generation admitted' if eval {workspace_fence({root=>'/synthetic',anchor=>'original',generation=>'a'x64,mode=>'dispatch'},undef,5);1;};
        }
        die 'stale machine dispatch side effects' unless join(',',@events) eq 'open,lock-and-verify,close-only';
    }
    for my $stop (0,1) {
        my $released=0; my $frame={pending=>'private-marker',until=>5};
        my $blocked=sub {$!=EAGAIN;return undef;};
        die 'blocked handshake released' unless handshake_step($frame,0,sub {0},$blocked,sub {$released++;})==0;
        die 'EOF/deadline handshake released' unless handshake_step($frame,$stop,sub {$stop ? 0 : 5},$blocked,sub {$released++;})==-1 && !$released;
        my @events; finish_owned(424242,1,{signal=>sub {push @events,'signal';},pause=>sub {},reap=>sub {push @events,'reap';}});
        die 'blocked handshake failed retirement' unless join(',',@events) eq 'signal,signal,reap';
    }
    {
        my $frame={pending=>'abc',until=>5}; my $sent='';my $released=0;
        for my $expected (0,0,1) {
            my $state=handshake_step($frame,0,sub {0},sub {$sent.=substr($_[0],0,1);return 1;},sub {$released++;});
            die 'partial handshake gate ordering' unless $state==$expected;
        }
        die 'incomplete handshake release' unless $sent eq 'abc' && $released==1;
        my $ticks=0;
        my $ok=eval {emit(undef,['READY'],3,{now=>sub {$ticks},pause=>sub {},write=>sub {$ticks++;$!=EAGAIN;return undef;}});1;};
        die 'unbounded ready emission' if $ok || $ticks!=3;
    }
    my $frames_input="A oldnonce\n{\"nonce\":\"next\"}\nP ping\n";
    for my $split (0..length($frames_input)) {
        my @chunks=grep {length($_)} (substr($frames_input,0,$split),substr($frames_input,$split));
        my $state={pending=>''}; my @frames;
        push @frames, frame_read($state,sub {shift @chunks;}) for 1..3;
        die 'coalesced frame loss' unless join('',@frames) eq $frames_input;
        die 'unexpected trailing frame' if defined(frame_read($state,sub {shift @chunks;}));
    }
    die 'incomplete output reported success' unless delivery_code(0,0)==125 && delivery_code(0,1)==0 && delivery_code(7,1)==7;
    {
        my @failed;
        my $ops={signal=>sub {push @failed,'signal'; die 'refused';},pause=>sub {},reap=>sub {push @failed,'reap';}};
        my $ok=eval {finish_owned(424242,1,$ops);1;};
        die 'double refusal skipped reap' if $ok || join(',',@failed) ne 'signal,signal,reap';
    }
    my @retired; my $held; my $pathname='original';
    my $retiring={v=>1,nonce=>'a',path=>'/tmp/gitcraft-owned-'.('b'x32),token=>'c'x64};
    my $retire_ops={pin=>sub {$held=$pathname; $_[1]->();},receipt=>sub {die 'missing atomic receipt';},unlink=>sub {push @retired,"$held/$_[0]"; $pathname='foreign-again';}};
    my $erased=eval {retire($retiring,$retire_ops);1;};
    die 'retired without proof' if $erased || @retired;
    $retire_ops->{receipt}=sub {$pathname='foreign';};
    retire($retiring,$retire_ops);
    die 'retirement escaped pinned identity' unless join(',',@retired) eq 'original/input,original/control,original/terminal';
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
    print "15 recording-only owner gates passed\n";
}
else { die 'unknown owner operation'; }

# Recording only: all locks/descriptors are scalar-memory handles. Production functions
# are loaded with kernel flock/close/truncate replaced before compilation.
use strict;use warnings;no warnings 'once';use Scalar::Util qw(refaddr);use Fcntl qw(LOCK_EX LOCK_NB LOCK_UN);
our ($flock,$closer,$truncate);
BEGIN {
    *CORE::GLOBAL::flock=sub (*$) {die 'unrecorded lock' unless $flock;return $flock->(@_);};
    *CORE::GLOBAL::close=sub (*) {return $closer ? $closer->(@_) : CORE::close($_[0]);};
    *CORE::GLOBAL::truncate=sub (*$) {die 'unrecorded truncate' unless $truncate;return $truncate->(@_);};
}
local @ARGV=('recording');do './src/remote/supervisor/owner.pl';die $@ if $@;
my $A='a'x64;my $B='b'x64;my $count=0;
sub ok {die "schedule assertion: $_[1]" unless $_[0];}
sub world {
    my ($body)=@_;my $w={owners=>{},next=>0,generation=>'',ids=>[qw(directory primary fence generation)],events=>[],waits=>0,handles=>{}};
    my $existing=sub {my ($root)=@_;my $id=++$w->{next};my @files;for my $kind (qw(primary fence generation)) {my $bytes='';open(my $f,'+>',\$bytes) or die 'memory handle';$w->{handles}{refaddr($f)}=[$kind,$id,\$bytes];push @files,$f;}return {root=>$root,files=>\@files,anchor_id=>'original',id=>$id,initialized=>0};};
    local $flock=sub {my ($f,$flags)=@_;my ($kind,$id)=@{$w->{handles}{refaddr($f)}};if ($flags&LOCK_UN) {delete $w->{owners}{$kind} if ($w->{owners}{$kind}//0)==$id;return 1;}if (exists $w->{owners}{$kind} && $w->{owners}{$kind}!=$id) {$!=Errno::EAGAIN();return 0;} $w->{owners}{$kind}=$id;return 1;};
    local $closer=sub {my ($f)=@_;my $entry=$w->{handles}{refaddr($f)};if ($entry) {my ($kind,$id)=@$entry;delete $w->{owners}{$kind} if ($w->{owners}{$kind}//0)==$id;push @{$w->{events}},'close-'.$kind;}return CORE::close($f);};
    local $truncate=sub {return 1;};
    no warnings 'redefine';
    local *workspace_existing=$existing;
    local *workspace_verify=sub {workspace_namespace([qw(directory primary fence generation)],$w->{ids});};
    local *workspace_wait=sub {die "recorded timeout\n" if ++$w->{waits}>4;};
    local *workspace_read=sub {my $entry=$w->{handles}{refaddr($_[0])};return ${$entry->[2]};};
    local *IO::Handle::sync=sub {1;};
    local *workspace_relative=sub {$_[1]->();};
    $w->{claim}=sub {my ($g)=@_;$w->{waits}=0;return workspace_claim({root=>'/synthetic',generation=>$g},undef,5);};
    $w->{grant}=sub {return {root=>'/synthetic',anchor=>'original',generation=>($_[0]//$A),mode=>($_[1]//'dispatch')};};
    # Read the actual published generation memory buffer for every newly opened descriptor.
    local *workspace_read=sub {return $w->{generation};};
    local *IO::Handle::flush=sub {my $entry=$w->{handles}{refaddr($_[0])};$w->{generation}=${$entry->[2]} if $entry && $entry->[0] eq 'generation';return 1;};
    $body->($w);$count++;
}
world(sub {my ($w)=@_;my $winner=$w->{claim}->($A);my $events=@{$w->{events}};ok(!eval {$w->{claim}->($B);1;},'loser admitted');ok($w->{owners}{primary}==$winner->{id},'winner touched');ok(!(grep {!/^close-/} @{$w->{events}}[$events..$#{$w->{events}}]),'loser mutation');workspace_close($winner);});
world(sub {my ($w)=@_;my $winner=$w->{claim}->($A);for my $clock (-999999,0,999999) {ok(!eval {workspace_claim({root=>'/synthetic',generation=>$B,pid=>424242,heartbeat=>$clock},undef,$clock);1;},'PID/time takeover');}workspace_close($winner);});
world(sub {my ($w)=@_;my $old=$w->{claim}->($A);workspace_close($old);my $new=$w->{claim}->($B);my $effects=0;no warnings 'redefine';local *run_inner=sub {$effects++;};ok(!eval {run({workspace=>$w->{grant}->($A),record=>'/synthetic/old',milliseconds=>1000},undef,1);1;},'queued old G');ok(!$effects,'old dispatch');workspace_close($new);});
world(sub {my ($w)=@_;my $claim=$w->{claim}->($A);no warnings 'redefine';local *run_inner=sub {workspace_close($claim);ok(!eval {$w->{claim}->($B);1;},'turnover crossed mutation');return (0,[]);};run({workspace=>$w->{grant}->($A,'mutation'),record=>'/synthetic/mut',milliseconds=>1000},undef,1);my $new=$w->{claim}->($B);workspace_close($new);});
world(sub {my ($w)=@_;my $claim=$w->{claim}->($A);my $locks=workspace_fence($w->{grant}->(),undef,5);for my $failure (qw(collision write flush sync close)) {my @events;no warnings 'redefine';local *atomic=sub {push @events,$failure;die "recorded $failure";};ok(!eval {workspace_publish($locks,'/synthetic/'.$A.'-dispatch-'.('d'x32).'.owner',{});push @events,'start';1;},'publication failure admitted');ok(join(',',@events) eq $failure,'late start');}workspace_close($locks);workspace_close($claim);});
{my $frame={pending=>'identity',until=>5};my $starts=0;for (1..4) {ok(handshake_step($frame,1,sub {0},sub {length($_[0])},sub {$starts++;})==-1,'pending gate reopened');}ok(!$starts,'late pending start');$count++;}
world(sub {my ($w)=@_;my $claim=$w->{claim}->($A);my $locks=workspace_fence($w->{grant}->(),undef,5);workspace_close($claim);my @records;for my $phase (qw(before partial complete)) {ok(!eval {$w->{claim}->($B);1;},'inventory crossed publication');push @records,'durable-owner' if $phase eq 'complete';}workspace_close($locks);my $new=$w->{claim}->($B);ok(join(',',@records) eq 'durable-owner','record omitted');workspace_close($new);});
{my $owner={v=>1,nonce=>'a',path=>'/tmp/gitcraft-owned-'.('b'x32),token=>'c'x64};for my $bad ('','{',encode({%$owner,state=>'stopped',code=>0,complete=>JSON::PP::true(),extra=>1})) {ok(!eval {checked_terminal($owner,$bad);1;},'unknown proof accepted');}$count++;}
world(sub {my ($w)=@_;my $old=$w->{claim}->($A);$w->{ids}[1]='replacement';ok(!eval {workspace_fence($w->{grant}->(),undef,5);1;},'replaced lock admitted');workspace_close($old);ok($w->{generation} eq "$A\n",'stale close rewrote G');ok(!eval {$w->{claim}->($B);1;},'split namespace');});
{for my $out ([1,1],[0,0],[125,0]) {ok(!cleanup_complete_allowed(@$out),'failed CLEAR completed');}$count++;}
sub ledger_case {
    my ($failure)=@_;my $owner={v=>1,nonce=>'a',path=>'/tmp/gitcraft-owned-'.('b'x32),token=>'c'x64};my $name="$A-mutation-".('d'x32).'.owner';my $record=encode({v=>3,anchor=>'original',generation=>$A,kind=>'mutation',endpoint=>$owner});my $receipt=encode({%$owner,state=>'stopped',code=>0,complete=>JSON::PP::true()});my $proof={name=>$name,record=>MIME::Base64::encode_base64($record,''),receipt=>MIME::Base64::encode_base64($receipt,'')};my $entry=ledger_decode($proof,'original');$entry->{present}=1;my $p={grant=>{anchor=>'original',generation=>$B},own=>"$B-mutation-".('e'x32).'.owner',entries=>{$name=>$entry},manifests=>[]};my %store=($name=>$record,"$name.proof"=>encode($proof));my @events;
    no warnings 'redefine';local *ledger_active=sub {die 'lease lost' if $failure eq 'lease';};local *ledger_verify=sub {push @events,'verify';};local *ledger_publish=sub {$store{$_[1]}=$_[2];push @events,'manifest';};local *ledger_retire_proven=sub {push @events,'retire';die 'refused' if $failure eq 'retire';};local *ledger_raw_read=sub {$store{$_[1]};};local *ledger_remove=sub {my ($p,$name,$bytes)=@_;push @events,'prune';die 'partial prune' if $failure eq 'prune' && $name=~/\.proof$/;ok($store{$name} eq $bytes,'changed proof');delete $store{$name};};
    ok(!eval {ledger_complete($p);1;},'partial ledger became success');if ($failure eq 'prune') {my @m=grep {/\.cleaned$/} keys %store;ok(@m==1 && index($store{$m[0]},MIME::Base64::encode_base64($record,''))>=0,'partial prune lost manifest');}else {ok(exists $store{$name},'failure lost record');ok(!(grep {$_ eq 'prune'} @events),'failure pruned evidence');}
}
ledger_case('prune');$count++;
ledger_case('retire');ledger_case('lease');$count++;
print "$count named workspace recording schedules passed\n";

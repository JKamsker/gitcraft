# R3-B1: intercept unlink before compiling production code; all pin/current-file/absence
# operations below are synthetic. No kernel filesystem mutation, process, lock or signal.
use strict;use warnings;no warnings 'once';
our (@events,%files,$refuse);
BEGIN {*CORE::GLOBAL::unlink=sub {my ($name)=@_;push @events,"unlink-$name";if (defined($refuse)&&$refuse eq $name) {$!=Errno::EACCES();return 0;}delete $files{$name};return 1;};}
local @ARGV=('recording');do './src/remote/supervisor/owner.pl';die $@ if $@;
my $owner={v=>1,nonce=>'a',path=>'/tmp/gitcraft-owned-'.('b'x32),token=>'c'x64};
my $proof=encode({%$owner,state=>'stopped',code=>0,complete=>JSON::PP::true()});
my $missing=0;my $count=0;
no warnings 'redefine';
local *pinned_directory=sub {push @events,'pin';return 0 if $missing;$_[1]->();return 1;};
local *terminal=sub {push @events,'read-current';return undef unless exists $files{terminal};return checked_terminal($owner,$files{terminal});};
local *ledger_artifacts_absent=sub {push @events,'check-absent';return !keys %files;};
sub fixture_reset {my ($terminal,$present)=@_;%files=$present ? (input=>'data',control=>'endpoint'):();$files{terminal}=$terminal if defined($terminal);@events=();$refuse=undef;$missing=0;}
sub check {die "recording assertion: $_[1]" unless $_[0];}
fixture_reset(encode({%$owner,state=>'stopped',code=>0,complete=>JSON::PP::true(),token=>'f'x64}),1);
check(!eval {retire($owner);1;},'ordinary retirement foreign negative control');check(join(',',@events) eq 'pin,read-current' && keys(%files)==3,'ordinary foreign deletion');
for my $terminal (encode({%$owner,state=>'stopped',code=>0,complete=>JSON::PP::true(),token=>'f'x64}),encode({%$owner,state=>'stopped',code=>7,complete=>JSON::PP::true()}),'{') {
 fixture_reset($terminal,1);check(!eval {ledger_retire_proven($owner,$proof);1;},'foreign receipt admitted');check(join(',',@events) eq 'pin,read-current','foreign deletion');check(keys(%files)==3,'foreign files lost');
}$count++;
fixture_reset(undef,1);check(!eval {ledger_retire_proven($owner,$proof);1;},'missing terminal admitted');check(join(',',@events) eq 'pin,read-current,check-absent' && keys(%files)==2,'missing-terminal deletion');$count++;
fixture_reset($proof,1);$missing=1;ledger_retire_proven($owner,$proof);check(join(',',@events) eq 'pin','absent directory mutation');$count++;
fixture_reset(undef,0);ledger_retire_proven($owner,$proof);check(join(',',@events) eq 'pin,read-current,check-absent','empty directory mutation');$count++;
fixture_reset($proof,1);ledger_retire_proven($owner,$proof);check(!keys(%files) && join(',',@events) eq 'pin,read-current,unlink-input,unlink-control,unlink-terminal','authenticated retirement');@events=();ledger_retire_proven($owner,$proof);check(join(',',@events) eq 'pin,read-current,check-absent','repeat deletion');$count++;
fixture_reset($proof,1);$refuse='control';check(!eval {ledger_retire_proven($owner,$proof);1;},'partial retire succeeded');check(exists($files{terminal})&&!exists($files{input}),'partial evidence lost');$refuse=undef;@events=();ledger_retire_proven($owner,$proof);check(!keys(%files)&&join(',',@events) eq 'pin,read-current,unlink-input,unlink-control,unlink-terminal','retry not authenticated');$count++;
{
 fixture_reset(encode({%$owner,state=>'stopped',code=>0,complete=>JSON::PP::true(),token=>'f'x64}),1);
 my $g='d'x64;my $name="$g-mutation-".('e'x32).'.owner';my $record=encode({v=>3,anchor=>'original',generation=>$g,kind=>'mutation',endpoint=>$owner});my %store=($name=>$record);my @stages;
 my $p={grant=>{anchor=>'original',generation=>$g},own=>"$g-mutation-".('f'x32).'.owner',entries=>{$name=>{name=>$name,record=>$record,receipt=>$proof,owner=>$owner,present=>1}},manifests=>[]};
 local *ledger_active=sub {};local *ledger_verify=sub {push @stages,'verify';};local *ledger_publish=sub {$store{$_[1]}=$_[2];push @stages,'manifest';};local *ledger_remove=sub {die 'must retain evidence';};
 check(!eval {ledger_complete($p);1;},'foreign directory completed ledger');check(join(',',@stages) eq 'verify,manifest' && $store{$name} eq $record,'manifest/record lost');check((scalar grep {/\.cleaned$/} keys %store)==1,'manifest absent');check(!(grep {/^unlink-/} @events),'foreign artifact removed');
}$count++;
print "$count cached retirement recording cases passed\n";

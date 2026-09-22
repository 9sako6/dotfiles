# Only the fixed dotfiles socket is owned here. Preserve externally supplied agents.
if [ -z "${SSH_AUTH_SOCK:-}" ] || [ "$SSH_AUTH_SOCK" = "$HOME/.ssh/agent.sock" ]; then
  if /usr/bin/env -u PERL5OPT -u PERL5LIB -u PERLLIB /usr/bin/perl - "$HOME/.ssh/agent.sock" <<'PERL'
use strict;
use warnings;
use Fcntl qw(:DEFAULT :flock :mode O_NOFOLLOW);
use File::Basename qw(dirname);

sub quiet_status {
    my $pid = fork();
    defined($pid) or die "ssh-agent: cannot fork: $!\n";
    if (!$pid) {
        open(STDOUT, '>', '/dev/null') or exit 127;
        open(STDERR, '>&', \*STDOUT) or exit 127;
        exec { $_[0] } @_ or exit 127;
    }
    waitpid($pid, 0) == $pid or die "ssh-agent: cannot inspect command result: $!\n";
    return $?;
}

sub reachable {
    my $status = quiet_status('ssh-add', '-l');
    return 1 if $status == 0 || $status == 256;
    return 0 if $status == 512;
    die "ssh-agent: identity probe failed (status $status)\n";
}

my $socket = shift @ARGV;
my $directory = dirname($socket);
if (!-e $directory) {
    mkdir($directory, 0700) or -d $directory or die "ssh-agent: cannot create SSH directory: $!\n";
}
my @directory = lstat($directory);
@directory && S_ISDIR($directory[2]) && $directory[4] == $<
    or die "ssh-agent: refusing an unowned or linked SSH directory\n";
sysopen(my $lock, "$directory/.dotfiles-agent.lock", O_RDWR | O_CREAT | O_NOFOLLOW, 0600)
    or die "ssh-agent: cannot open startup lock: $!\n";
my @lock = stat($lock);
S_ISREG($lock[2]) && $lock[4] == $< or die "ssh-agent: invalid startup lock\n";
flock($lock, LOCK_EX) or die "ssh-agent: cannot lock startup: $!\n";
$ENV{SSH_AUTH_SOCK} = $socket;
my @socket = lstat($socket);
if (@socket) {
    S_ISSOCK($socket[2]) && $socket[4] == $<
        or die "ssh-agent: refusing to replace an unowned or non-socket path\n";
    exit 0 if reachable();
    unlink($socket) or die "ssh-agent: cannot remove stale owned socket: $!\n";
} elsif (!$!{ENOENT}) {
    die "ssh-agent: cannot inspect socket: $!\n";
}
quiet_status('ssh-agent', '-a', $socket, '-s') == 0
    or die "ssh-agent: could not start the managed agent\n";
reachable() or die "ssh-agent: started agent is not reachable\n";
PERL
  then
    export SSH_AUTH_SOCK="$HOME/.ssh/agent.sock"
  else
    printf '%s\n' 'dotfiles: managed SSH agent is unavailable; existing paths were not replaced unless they were owned stale sockets' >&2
  fi
fi

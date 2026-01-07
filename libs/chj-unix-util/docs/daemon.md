# daemon.rs

## General design

- Do *not* have "want" versus "is" state, just one state: start the
  daemon, presumed running. start it, and if it gets killed, it will be
  stopped again. Keep it simple. (Instead, use `forking_loop` with a
  small and reliable parent to minimize the risk of getting killed.)

- Do *not* (currently) have a master (cygote) process: we start the
  daemon from the current environment. Just be careful what
  environment parts might be relevant. Keep it simple. Maybe implement
  it later.

- Have a single directory with all info: state files
  (`daemon_is_running.lock` and `pid` files), and a subdirectory with
  logs.
  

## Locking design

Want to prevent the following problems:

- daemon killed by whatever means must be detectable as the daemon now
  being stopped

- daemon starting needs to take the lock so that no 2 daemons are
  starting at the same time

- between taking that lock and updating the pid file, there must be no
  race where a `--daemon stop` could use an outdated pid

- daemon conflicting with non-daemon runs will be detected and lead to
  error messages, but that should not prevent the daemon from being
  started or leading to it quitting. Using a different lock here (a
  third one besides the two from the daemon infrastructure).

Thus:

- Use flock, since those locks are released automatically when a
  process quits, the machine reboots etc. (Drawback: buggy
  implementations, only in the past?)
  
- Use two files: `daemon_is_running.lock` just for the running status,
  and a separate `pid` file which is `flock`ed separately for its
  updating.

- For daemon start, lock the `pid` file exclusively before taking the
  `is_running` lock. Only ever take pid file locks for a short
  time. For daemon stop, take the `pid` file lock (shared) after
  checking `is_running`.


## Logging design

- Have a `logs` folder with files named `current.log` or numbered
  like `00001.log`, that lies inside the base folder.

- Do not use compression, keep it simple.

- Do not require the daemon to use special logging infrastructure,
  just print to stderr and stdout. Pipe those filehandles to a process
  that writes the messages down as logfiles.

- Prepend a human-readable time stamp, and a "\t" to every line.

- A logfile is rotated when it reaches a predefined size (not age).

- When there are more than a predefined number of log files, delete
  the oldest (lowest-numbered).


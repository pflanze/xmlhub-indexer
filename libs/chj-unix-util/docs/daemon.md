# daemon.rs

## General design

- Choice of graceful and forced stopping modes: graceful just informs
  the daemon that it should stop at the next checkpoint. Forced is via
  unix signals (first TERM then KILL). The choice can be given via
  clap subcommand options, or action prefixes in FromStr; without
  explicitly naming one of the choices (or when naming both,
  cancelling each other), the application-provided default is used.

- Graceful stopping and restarting works via a want state in a mmap'ed
  state file (that also contains the pid and is also used for the
  forced mode). The application (in the daemon callback function) must
  periodically check if it should stop, and if so just return from the
  callback. The application receives a ExecutionResult, on which it
  must call `daemon_cleanup()` (or it will panic in Drop!). This
  method carries out the re-exec in case of a restart action. Thus the
  application must place this call high up, ideally in the main
  function after everything relevant has been cleaned up (Drop actions
  ran).

- There is a feature to restart the daemon on failures (settings via
  `RestartOnFailures`). This does another fork before running the
  workload--i.e. it creates a service observer daemon at the same time
  as the daemon itself, and is torn down again at the same time as the
  daemon, too (in either soft or hard mode).

- There is currently no master (cygote) process: we start the daemon
  from the current environment, with the tradeoff that this implies
  (you can set up the environment that the daemon sees, but also
  *have* to control it). Note that with graceful restarts, the daemon
  does *not* pick up the environment or command line argument changes
  of the process that calls for the restart: the daemon re-exec's
  itself with its existing environment and command line arguments. If
  that's problematic, consider making forced actions the default, and
  document that the explicit soft actions have this
  behaviour. (Future: add option to disable soft actions altogether?)

- There is one file, `daemon_state.mmap`, that serves both to record
  the pid and wanted state, as well as to put the flock on that
  indicates that a daemon is running. Note: `run` mode does not take
  that lock (it does not use that file at all; the assumption is that
  whatever daemon service starts the process also controls that only
  one is running)!

- There is a separate path to a directory where logs (with log
  rotation) are written to. There is (currently) no log compression.


## Locking design

Want to prevent the following problems:

- daemon killed by whatever means must be detectable as the daemon now
  being stopped

- starting a daemon needs to take the lock so that no 2 daemons are
  ever running at the same time

- between taking that lock and updating the pid file, there must be no
  race where a `--daemon stop` could use an outdated pid

Thus:

- Use flock, since those locks are released automatically when a
  process quits, the machine reboots etc. (Drawback: buggy
  implementations, only in the past?)

- Update the state file with the pid and want status atomically.


## Logging design

- Have a `logs` folder with files named `current.log` or numbered like
  `000001.log`, that lies inside the log directory.

- Do not use compression, keep it simple.

- Do not require the daemon to use special logging infrastructure,
  just print to stderr and stdout. Pipe those filehandles to a process
  that writes the messages down as logfiles.

- Prepend a human-readable time stamp, and a "\t" to every line, if
  enabled in the `Daemon` config.

- A logfile is rotated when it reaches a predefined size (not age).

- When there are more than a predefined number of log files, delete
  the oldest (lowest-numbered) if so configured (or never, by
  default).


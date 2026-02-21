const os = require('os');
const pty = require('node-pty');

// Configuration from environment or defaults
const shell = process.env.SHELL_PATH || (os.platform() === 'win32' ? 'powershell.exe' : '/bin/bash');
const initialCols = parseInt(process.env.PTY_COLS || '80');
const initialRows = parseInt(process.env.PTY_ROWS || '24');
const cwd = process.env.PTY_CWD || process.cwd();

const ptyProcess = pty.spawn(shell, [], {
  name: 'xterm-color',
  cols: initialCols,
  rows: initialRows,
  cwd: cwd,
  env: process.env
});

// Output from the PTY goes directly to stdout
ptyProcess.onData((data) => {
  // Use process.stdout.write which is the standard, well-integrated way for stream outputs
  // On Windows pipes, fs.writeSync(1, ...) can sometimes behave unpredictably.
  process.stdout.write(data);
});

// Process exit
ptyProcess.onExit(({ exitCode, signal }) => {
  process.exit(exitCode || 0);
});

// Control messages from Python over stdin
let inputBuffer = '';
process.stdin.on('data', (chunk) => {
  inputBuffer += chunk.toString('utf8');
  
  let lineEnd;
  while ((lineEnd = inputBuffer.indexOf('\n')) !== -1) {
    const line = inputBuffer.substring(0, lineEnd).trim();
    inputBuffer = inputBuffer.substring(lineEnd + 1);
    
    if (!line) continue;

    try {
      const msg = JSON.parse(line);
      if (msg.type === 'input') {
        // Use a small pause if writing very large chunks to avoid pty buffer overflow
        ptyProcess.write(msg.data);
      } else if (msg.type === 'resize') {
        if (msg.cols > 0 && msg.rows > 0) {
          ptyProcess.resize(msg.cols, msg.rows);
        }
      }
    } catch (e) {
      // Ignore parse errors from noise on stdin
    }
  }
});

// Ensure we don't crash on pipe closure
process.stdout.on('error', (err) => {
  if (err.code === 'EPIPE') process.exit(0);
});

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
  process.stdout.write(data);
});

// Process exit
ptyProcess.onExit(({ exitCode, signal }) => {
  process.exit(exitCode || 0);
});

// Control messages from Python over stdin
// We expect a line-delimited JSON format for control messages
let inputBuffer = '';
process.stdin.on('data', (chunk) => {
  inputBuffer += chunk.toString();
  
  let lineEnd;
  while ((lineEnd = inputBuffer.indexOf('\n')) !== -1) {
    const line = inputBuffer.substring(0, lineEnd).trim();
    inputBuffer = inputBuffer.substring(lineEnd + 1);
    
    if (!line) continue;

    try {
      const msg = JSON.parse(line);
      if (msg.type === 'input') {
        ptyProcess.write(msg.data);
      } else if (msg.type === 'resize') {
        ptyProcess.resize(msg.cols, msg.rows);
      }
    } catch (e) {
      // If it's not JSON, we might have received raw input by mistake?
      // Log it or ignore for stability.
      console.error('Invalid JSON message from parent:', line);
    }
  }
});

// Ensure we don't crash on pipe closure
process.stdout.on('error', (err) => {
  if (err.code === 'EPIPE') process.exit(0);
});

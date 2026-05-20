export function platformLog(tag: string, level: 'debug' | 'info' | 'warn' | 'error', message: string, ...args: Object[]): void {
  const prefix = `[${tag}]`;
  const formattedArgs = args.length > 0 ? ' ' + args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ') : '';
  const fullMsg = `${prefix} ${message}${formattedArgs}`;
  switch (level) {
    case 'debug':
    case 'info':
      console.info(fullMsg);
      break;
    case 'warn':
      console.warn(fullMsg);
      break;
    case 'error':
      console.error(fullMsg);
      break;
  }
}

export function platformToast(message: string): void {
  AlertDialog.show({
    message: message,
    duration: 2000,
  });
}

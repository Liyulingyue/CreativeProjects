let audioCtx: AudioContext | null = null;

function getAudioContext(): AudioContext | null {
  if (typeof window === "undefined") return null;
  const Ctx = window.AudioContext || (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctx) return null;
  if (!audioCtx) {
    audioCtx = new Ctx();
  }
  return audioCtx;
}

function playToneSequence(sequence: Array<{ freq: number; duration: number; gap?: number }>): void {
  const ctx = getAudioContext();
  if (!ctx) return;

  if (ctx.state === "suspended") {
    ctx.resume().catch(() => {});
  }

  let t = ctx.currentTime;
  for (const step of sequence) {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();

    osc.type = "sine";
    osc.frequency.value = step.freq;

    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(0.08, t + 0.02);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + step.duration);

    osc.connect(gain);
    gain.connect(ctx.destination);

    osc.start(t);
    osc.stop(t + step.duration + 0.02);

    t += step.duration + (step.gap ?? 0.04);
  }
}

export function playBreakAlertSound(): void {
  playToneSequence([
    { freq: 660, duration: 0.13, gap: 0.05 },
    { freq: 520, duration: 0.2, gap: 0.06 },
    { freq: 420, duration: 0.24 },
  ]);
}

export function playWelcomeBackSound(): void {
  playToneSequence([
    { freq: 440, duration: 0.1, gap: 0.03 },
    { freq: 550, duration: 0.1, gap: 0.03 },
    { freq: 660, duration: 0.14 },
  ]);
}

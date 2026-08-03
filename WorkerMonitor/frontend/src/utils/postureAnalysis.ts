export interface Landmark {
  x: number;
  y: number;
  z: number;
  visibility?: number;
}

export interface PostureResult {
  score: number;
  headForward: boolean;
  headTilt: boolean;
  shoulderUneven: boolean;
  slouching: boolean;
  details: {
    headForwardAngle: number;
    headTiltAngle: number;
    shoulderDiff: number;
    slouchAngle: number;
  };
}

const HEAD_FORWARD_THRESHOLD = 15;
const HEAD_TILT_THRESHOLD = 10;
const SHOULDER_UNEVEN_THRESHOLD = 0.04;
const SLOUCH_THRESHOLD = 20;

export function analyzePosture(landmarks: Landmark[]): PostureResult {
  const nose = landmarks[0];
  const leftEar = landmarks[3];
  const rightEar = landmarks[4];
  const leftShoulder = landmarks[5];
  const rightShoulder = landmarks[6];
  const leftHip = landmarks[11];
  const rightHip = landmarks[12];

  const headForwardAngle = calcHeadForward(leftEar, rightEar, leftShoulder, rightShoulder);
  const headTiltAngle = calcHeadTilt(nose, leftEar, rightEar, leftShoulder, rightShoulder);
  const shoulderDiff = calcShoulderDiff(leftShoulder, rightShoulder);
  const slouchAngle = calcSlouch(leftShoulder, rightShoulder, leftHip, rightHip);

  const headForward = headForwardAngle > HEAD_FORWARD_THRESHOLD;
  const headTilt = headTiltAngle > HEAD_TILT_THRESHOLD;
  const shoulderUneven = shoulderDiff > SHOULDER_UNEVEN_THRESHOLD;
  const slouching = slouchAngle > SLOUCH_THRESHOLD;

  let penalty = 0;
  if (headForward) penalty += 25;
  if (headTilt) penalty += 20;
  if (shoulderUneven) penalty += 15;
  if (slouching) penalty += 30;

  const score = Math.max(0, 100 - penalty);

  return {
    score,
    headForward,
    headTilt,
    shoulderUneven,
    slouching,
    details: {
      headForwardAngle,
      headTiltAngle,
      shoulderDiff,
      slouchAngle,
    },
  };
}

function calcHeadForward(
  leftEar: Landmark,
  rightEar: Landmark,
  leftShoulder: Landmark,
  rightShoulder: Landmark
): number {
  const earX = (leftEar.x + rightEar.x) / 2;
  const shoulderX = (leftShoulder.x + rightShoulder.x) / 2;
  const earY = (leftEar.y + rightEar.y) / 2;
  const shoulderY = (leftShoulder.y + rightShoulder.y) / 2;
  const dy = Math.abs(earY - shoulderY);
  if (dy < 0.001) return 0;
  const dx = Math.abs(earX - shoulderX);
  return (Math.atan2(dx, dy) * 180) / Math.PI;
}

function calcHeadTilt(
  nose: Landmark,
  leftEar: Landmark,
  rightEar: Landmark,
  leftShoulder: Landmark,
  rightShoulder: Landmark
): number {
  const midShoulderX = (leftShoulder.x + rightShoulder.x) / 2;
  const midShoulderY = (leftShoulder.y + rightShoulder.y) / 2;
  const midEarY = (leftEar.y + rightEar.y) / 2;
  const shoulderDx = rightShoulder.x - leftShoulder.x;
  const shoulderDy = rightShoulder.y - leftShoulder.y;
  const headDx = nose.x - midShoulderX;
  const headDy = nose.y - midShoulderY;
  const shoulderAngle = Math.atan2(shoulderDy, shoulderDx);
  const headAngle = Math.atan2(headDy, headDx);
  let diff = Math.abs(headAngle - shoulderAngle) * (180 / Math.PI);
  if (diff > 90) diff = 180 - diff;
  const earDiff = Math.abs((leftEar.y - midEarY) - (rightEar.y - midEarY));
  return diff * 0.5 + earDiff * 200;
}

function calcShoulderDiff(leftShoulder: Landmark, rightShoulder: Landmark): number {
  return Math.abs(leftShoulder.y - rightShoulder.y);
}

function calcSlouch(
  leftShoulder: Landmark,
  rightShoulder: Landmark,
  leftHip: Landmark,
  rightHip: Landmark
): number {
  const midShoulderX = (leftShoulder.x + rightShoulder.x) / 2;
  const midShoulderY = (leftShoulder.y + rightShoulder.y) / 2;
  const midHipX = (leftHip.x + rightHip.x) / 2;
  const midHipY = (leftHip.y + rightHip.y) / 2;
  const dx = Math.abs(midShoulderX - midHipX);
  const dy = Math.abs(midShoulderY - midHipY);
  if (dy < 0.001) return 0;
  return (Math.atan2(dx, dy) * 180) / Math.PI;
}

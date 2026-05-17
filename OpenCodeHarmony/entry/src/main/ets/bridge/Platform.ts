/**
 * 平台检测工具
 * 用于判断当前运行平台，实现跨平台差异化处理
 */

// 平台类型枚举
export enum PlatformType {
  HARMONYOS = 'HarmonyOS',
  ANDROID = 'Android',
  IOS = 'iOS',
  UNKNOWN = 'Unknown'
}

/**
 * 获取当前运行平台
 * @returns PlatformType 平台类型
 */
export function getPlatform(): PlatformType {
  // @ts-ignore: deviceInfo 在跨平台编译时可能报错
  const deviceInfo = require('@ohos.deviceInfo');
  
  // @ts-ignore
  const osName = deviceInfo?.osFullName || '';
  
  if (osName.includes('HarmonyOS') || osName.includes('OpenHarmony')) {
    return PlatformType.HARMONYOS;
  } else if (osName.includes('Android')) {
    return PlatformType.ANDROID;
  } else if (osName.includes('iOS')) {
    return PlatformType.IOS;
  }
  
  return PlatformType.UNKNOWN;
}

/**
 * 判断是否为 HarmonyOS 平台
 */
export function isHarmonyOS(): boolean {
  return getPlatform() === PlatformType.HARMONYOS;
}

/**
 * 判断是否为 Android 平台
 */
export function isAndroid(): boolean {
  return getPlatform() === PlatformType.ANDROID;
}

/**
 * 判断是否为 iOS 平台
 */
export function isIOS(): boolean {
  return getPlatform() === PlatformType.IOS;
}

/**
 * 平台相关日志输出
 */
export class PlatformLogger {
  private static TAG = '[CrossPlatform]';
  
  static info(message: string): void {
    console.info(`${this.TAG} [${getPlatform()}] ${message}`);
  }
  
  static error(message: string): void {
    console.error(`${this.TAG} [${getPlatform()}] ${message}`);
  }
  
  static warn(message: string): void {
    console.warn(`${this.TAG} [${getPlatform()}] ${message}`);
  }
}

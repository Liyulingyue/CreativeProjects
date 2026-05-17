/**
 * Bridge 平台桥接基类
 * 用于 ArkUI 与 Android/iOS 原生层之间的通信
 */

import { PlatformType, getPlatform, isHarmonyOS, PlatformLogger } from './Platform';

// Bridge 回调类型
export type BridgeCallback = (result: BridgeResult) => void;

// Bridge 结果类型
export interface BridgeResult {
  code: number;
  message: string;
  data: Record<string, Object> | null;
}

// Bridge 配置
export interface BridgeConfig {
  moduleName: string;    // 原生模块名称
  methodName: string;    // 原生方法名称
  params?: Record<string, Object>;  // 参数
}

/**
 * Bridge 平台桥接类
 * 
 * 使用说明：
 * - HarmonyOS 平台：直接调用 ArkTS API
 * - Android 平台：通过 BridgePlugin 调用 Java/Kotlin
 * - iOS 平台：通过 BridgePlugin 调用 Swift/Objective-C
 */
export class Bridge {
  private static instance: Bridge | null = null;
  private nativeBridge: Object | null = null;
  
  private constructor() {
    this.initNativeBridge();
  }
  
  /**
   * 获取 Bridge 单例
   */
  static getInstance(): Bridge {
    if (!Bridge.instance) {
      Bridge.instance = new Bridge();
    }
    return Bridge.instance;
  }
  
  /**
   * 初始化原生 Bridge
   */
  private initNativeBridge(): void {
    if (isHarmonyOS()) {
      // HarmonyOS 平台不需要 Bridge
      PlatformLogger.info('Running on HarmonyOS, native bridge not needed');
      return;
    }
    
    // @ts-ignore: 跨平台编译时可能报错
    try {
      // Android/iOS 平台需要加载原生 Bridge
      // 实际实现需要在 .arkui-x/android 和 .arkui-x/ios 中配置
      PlatformLogger.info('Initializing native bridge for ' + getPlatform());
    } catch (err) {
      PlatformLogger.error('Failed to init native bridge: ' + JSON.stringify(err));
    }
  }
  
  /**
   * 调用原生方法
   * @param config Bridge 配置
   * @param callback 回调函数
   */
  call(config: BridgeConfig, callback?: BridgeCallback): void {
    if (isHarmonyOS()) {
      // HarmonyOS 平台不应该通过 Bridge 调用
      const error: BridgeResult = {
        code: -1,
        message: 'Bridge should not be used on HarmonyOS platform',
        data: null
      };
      callback?.(error);
      PlatformLogger.warn(error.message);
      return;
    }
    
    PlatformLogger.info(`Calling native method: ${config.moduleName}.${config.methodName}`);
    
    // @ts-ignore: 跨平台编译
    try {
      // 这里需要根据实际平台实现
      // Android: 通过 BridgePlugin.callMethod
      // iOS: 通过 BridgePlugin.callMethod
      const result: BridgeResult = {
        code: 0,
        message: 'success',
        data: null
      };
      callback?.(result);
    } catch (err) {
      const error: BridgeResult = {
        code: -1,
        message: JSON.stringify(err),
        data: null
      };
      callback?.(error);
      PlatformLogger.error('Bridge call failed: ' + error.message);
    }
  }
  
  /**
   * 同步调用原生方法（仅支持部分场景）
   * @param config Bridge 配置
   * @returns Bridge 结果
   */
  callSync(config: BridgeConfig): BridgeResult {
    if (isHarmonyOS()) {
      return {
        code: -1,
        message: 'Bridge should not be used on HarmonyOS platform',
        data: null
      };
    }
    
    PlatformLogger.info(`Calling native method sync: ${config.moduleName}.${config.methodName}`);
    
    // @ts-ignore: 跨平台编译
    return {
      code: 0,
      message: 'success',
      data: null
    };
  }
  
  /**
   * 注册原生层回调
   * @param eventName 事件名称
   * @param callback 回调函数
   */
  registerCallback(eventName: string, callback: BridgeCallback): void {
    PlatformLogger.info(`Registering callback for event: ${eventName}`);
    // 实现原生层向 ArkUI 发送事件的能力
  }
  
  /**
   * 注销原生层回调
   * @param eventName 事件名称
   */
  unregisterCallback(eventName: string): void {
    PlatformLogger.info(`Unregistering callback for event: ${eventName}`);
  }
}

// 导出便捷方法
export const bridge = Bridge.getInstance();

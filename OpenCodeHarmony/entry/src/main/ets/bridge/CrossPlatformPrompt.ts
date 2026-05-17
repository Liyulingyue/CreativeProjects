/**
 * 跨平台提示框封装
 * 
 * HarmonyOS: 使用 @ohos.promptAction
 * Android: 使用 Toast / AlertDialog
 * iOS: 使用 Toast / UIAlertController
 */

import { isHarmonyOS, PlatformLogger } from './Platform';
import { bridge, BridgeResult } from './Bridge';

// Toast 位置
export enum ToastPosition {
  TOP = 'TOP',
  CENTER = 'CENTER',
  BOTTOM = 'BOTTOM'
}

// Toast 配置
export interface ToastConfig {
  message: string;
  duration?: number;  // 显示时长（毫秒）
  position?: ToastPosition;
  bottom?: number;  // 底部距离（仅 BOTTOM 位置有效）
}

// 对话框按钮
export interface DialogButton {
  text: string;
  color?: string;
  action?: () => void;
}

// 对话框配置
export interface DialogConfig {
  title?: string;
  message: string;
  buttons?: DialogButton[];
  cancelable?: boolean;  // 是否可取消
}

/**
 * 跨平台提示框类
 */
export class CrossPlatformPrompt {
  private static TAG = 'CrossPlatformPrompt';
  private static harmonyPrompt: Object | null = null;
  
  /**
   * 初始化 HarmonyOS Prompt
   */
  private static async initHarmonyPrompt(): Promise<void> {
    if (!isHarmonyOS()) return;
    
    if (!this.harmonyPrompt) {
      // @ts-ignore: 跨平台编译
      this.harmonyPrompt = await import('@ohos.promptAction');
    }
  }
  
  /**
   * 显示 Toast 提示
   * @param message 消息内容
   * @param duration 显示时长（毫秒）
   */
  static async showToast(message: string, duration?: number): Promise<void>;
  
  /**
   * 显示 Toast 提示（配置对象）
   * @param config Toast 配置
   */
  static async showToast(config: ToastConfig): Promise<void>;
  
  static async showToast(param: string | ToastConfig, duration?: number): Promise<void> {
    const config: ToastConfig = typeof param === 'string' 
      ? { message: param, duration }
      : param;
    
    PlatformLogger.info(`Show toast: ${config.message}`);
    
    if (isHarmonyOS()) {
      await this.harmonyShowToast(config);
    } else {
      await this.bridgeShowToast(config);
    }
  }
  
  /**
   * HarmonyOS Toast
   */
  private static async harmonyShowToast(config: ToastConfig): Promise<void> {
    await this.initHarmonyPrompt();
    
    // @ts-ignore
    if (!this.harmonyPrompt) {
      console.warn('Toast: ' + config.message);
      return;
    }
    
    try {
      const options = {
        message: config.message,
        duration: config.duration || 2000,
        bottom: config.bottom || 100
      };
      
      // @ts-ignore
      await this.harmonyPrompt.showToast(options);
    } catch (err) {
      PlatformLogger.error('Show toast failed: ' + JSON.stringify(err));
      console.warn('Toast: ' + config.message);
    }
  }
  
  /**
   * Bridge Toast
   */
  private static async bridgeShowToast(config: ToastConfig): Promise<void> {
    return new Promise((resolve) => {
      bridge.call({
        moduleName: 'Prompt',
        methodName: 'showToast',
        params: config
      }, () => {
        resolve();
      });
    });
  }
  
  /**
   * 显示对话框
   * @param config 对话框配置
   */
  static async showDialog(config: DialogConfig): Promise<number> {
    PlatformLogger.info(`Show dialog: ${config.title || ''} - ${config.message}`);
    
    if (isHarmonyOS()) {
      return this.harmonyShowDialog(config);
    } else {
      return this.bridgeShowDialog(config);
    }
  }
  
  /**
   * HarmonyOS 对话框
   */
  private static async harmonyShowDialog(config: DialogConfig): Promise<number> {
    await this.initHarmonyPrompt();
    
    // @ts-ignore
    if (!this.harmonyPrompt) {
      const confirmed = confirm(config.message);
      return confirmed ? 0 : -1;
    }
    
    return new Promise((resolve) => {
      const buttons = config.buttons?.map((btn, index) => ({
        text: btn.text,
        color: btn.color || '#000000'
      })) || [
        { text: '确定', color: '#007DFF' }
      ];
      
      const options = {
        title: config.title || '',
        message: config.message,
        buttons: buttons,
        cancel: config.cancelable !== false ? () => resolve(-1) : undefined
      };
      
      try {
        // @ts-ignore
        this.harmonyPrompt.showDialog(options, (error: Object, data: Object) => {
          // @ts-ignore
          if (error) {
            resolve(-1);
            return;
          }
          // @ts-ignore
          resolve(data?.index || 0);
        });
      } catch (err) {
        PlatformLogger.error('Show dialog failed: ' + JSON.stringify(err));
        resolve(-1);
      }
    });
  }
  
  /**
   * Bridge 对话框
   */
  private static async bridgeShowDialog(config: DialogConfig): Promise<number> {
    return new Promise((resolve) => {
      bridge.call({
        moduleName: 'Prompt',
        methodName: 'showDialog',
        params: config
      }, (result: BridgeResult) => {
        if (result.code === 0 && result.data) {
          resolve(result.data['buttonIndex'] as number || 0);
        } else {
          resolve(-1);
        }
      });
    });
  }
  
  /**
   * 显示确认对话框（便捷方法）
   * @param title 标题
   * @param message 消息
   * @param confirmText 确认按钮文本
   * @param cancelText 取消按钮文本
   * @returns 是否确认
   */
  static async confirm(
    title: string,
    message: string,
    confirmText: string = '确定',
    cancelText: string = '取消'
  ): Promise<boolean> {
    const result = await this.showDialog({
      title,
      message,
      buttons: [
        { text: cancelText, color: '#666666' },
        { text: confirmText, color: '#007DFF' }
      ],
      cancelable: true
    });
    
    // result 为按钮索引，1 表示确认
    return result === 1;
  }
  
  /**
   * 显示警告对话框
   * @param title 标题
   * @param message 消息
   * @param buttonText 按钮文本
   */
  static async alert(
    title: string,
    message: string,
    buttonText: string = '确定'
  ): Promise<void> {
    await this.showDialog({
      title,
      message,
      buttons: [
        { text: buttonText, color: '#007DFF' }
      ],
      cancelable: false
    });
  }
  
  /**
   * 显示加载提示
   * @param message 加载消息
   */
  static async showLoading(message: string = '加载中...'): Promise<void> {
    PlatformLogger.info(`Show loading: ${message}`);
    
    if (isHarmonyOS()) {
      await this.initHarmonyPrompt();
      
      // @ts-ignore
      if (!this.harmonyPrompt) {
        console.log('Loading: ' + message);
        return;
      }
      
      try {
        // @ts-ignore
        await this.harmonyPrompt.showDialog({
          message: message,
          buttons: []
        });
      } catch (err) {
        PlatformLogger.error('Show loading failed: ' + JSON.stringify(err));
      }
    } else {
      bridge.call({
        moduleName: 'Prompt',
        methodName: 'showLoading',
        params: { message }
      });
    }
  }
  
  /**
   * 隐藏加载提示
   */
  static async hideLoading(): Promise<void> {
    PlatformLogger.info('Hide loading');
    
    if (isHarmonyOS()) {
      // HarmonyOS 的 showDialog 没有专门的关闭方法
      // 需要通过返回的 Promise resolve 来关闭
    } else {
      bridge.call({
        moduleName: 'Prompt',
        methodName: 'hideLoading'
      });
    }
  }
}

// 导出便捷实例
export const prompt = CrossPlatformPrompt;

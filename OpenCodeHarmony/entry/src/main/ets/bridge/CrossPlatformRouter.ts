/**
 * 跨平台路由封装
 * 
 * HarmonyOS: 使用 @ohos.router
 * Android/iOS: 通过 Bridge 调用原生路由
 */

import { isHarmonyOS, PlatformLogger } from './Platform';
import { bridge, BridgeResult } from './Bridge';

// 路由参数类型
export interface RouterParams {
  url: string;
  params?: Record<string, Object>;
}

// 路由模式
export enum RouterMode {
  Standard = 'Standard',    // 标准模式，每次跳转创建新页面
  Single = 'Single'         // 单例模式，复用已存在的页面
}

/**
 * 跨平台路由类
 */
export class CrossPlatformRouter {
  private static TAG = 'CrossPlatformRouter';
  private static harmonyRouter: Object | null = null;
  
  /**
   * 初始化 HarmonyOS 路由
   */
  private static async initHarmonyRouter(): Promise<void> {
    if (!isHarmonyOS()) return;
    
    if (!this.harmonyRouter) {
      // @ts-ignore: 跨平台编译
      this.harmonyRouter = await import('@ohos.router');
    }
  }
  
  /**
   * 页面跳转（push）
   * @param url 目标页面路径
   * @param params 传递参数
   * @param mode 路由模式
   */
  static async push(
    url: string, 
    params?: Record<string, Object>,
    mode?: RouterMode
  ): Promise<void> {
    PlatformLogger.info(`Push to: ${url}`);
    
    if (isHarmonyOS()) {
      // HarmonyOS 平台使用原生路由
      await this.initHarmonyRouter();
      
      // @ts-ignore
      if (this.harmonyRouter) {
        const options = {
          url: url,
          params: params || {}
        };
        
        try {
          // @ts-ignore
          if (mode === RouterMode.Single) {
            // @ts-ignore
            await this.harmonyRouter.pushUrl(options, this.harmonyRouter.RouteMode.SINGLE);
          } else {
            // @ts-ignore
            await this.harmonyRouter.pushUrl(options);
          }
          PlatformLogger.info('Push success');
        } catch (err) {
          PlatformLogger.error('Push failed: ' + JSON.stringify(err));
          throw err;
        }
      }
    } else {
      // Android/iOS 平台通过 Bridge 调用
      return new Promise((resolve, reject) => {
        bridge.call({
          moduleName: 'Router',
          methodName: 'push',
          params: { url, params, mode }
        }, (result: BridgeResult) => {
          if (result.code === 0) {
            resolve();
          } else {
            reject(new Error(result.message));
          }
        });
      });
    }
  }
  
  /**
   * 页面替换（replace）
   * @param url 目标页面路径
   * @param params 传递参数
   */
  static async replace(
    url: string, 
    params?: Record<string, Object>
  ): Promise<void> {
    PlatformLogger.info(`Replace to: ${url}`);
    
    if (isHarmonyOS()) {
      await this.initHarmonyRouter();
      
      // @ts-ignore
      if (this.harmonyRouter) {
        const options = {
          url: url,
          params: params || {}
        };
        
        try {
          // @ts-ignore
          await this.harmonyRouter.replaceUrl(options);
          PlatformLogger.info('Replace success');
        } catch (err) {
          PlatformLogger.error('Replace failed: ' + JSON.stringify(err));
          throw err;
        }
      }
    } else {
      return new Promise((resolve, reject) => {
        bridge.call({
          moduleName: 'Router',
          methodName: 'replace',
          params: { url, params }
        }, (result: BridgeResult) => {
          if (result.code === 0) {
            resolve();
          } else {
            reject(new Error(result.message));
          }
        });
      });
    }
  }
  
  /**
   * 返回上一页（back）
   * @param options 返回选项
   */
  static async back(options?: { 
    steps?: number;  // 返回步数
    params?: Record<string, Object>;  // 返回携带参数
  }): Promise<void> {
    PlatformLogger.info(`Back, steps: ${options?.steps || 1}`);
    
    if (isHarmonyOS()) {
      await this.initHarmonyRouter();
      
      // @ts-ignore
      if (this.harmonyRouter) {
        try {
          if (options?.params) {
            // @ts-ignore
            await this.harmonyRouter.back(options.params);
          } else {
            // @ts-ignore
            await this.harmonyRouter.back();
          }
          PlatformLogger.info('Back success');
        } catch (err) {
          PlatformLogger.error('Back failed: ' + JSON.stringify(err));
          throw err;
        }
      }
    } else {
      return new Promise((resolve, reject) => {
        bridge.call({
          moduleName: 'Router',
          methodName: 'back',
          params: options || {}
        }, (result: BridgeResult) => {
          if (result.code === 0) {
            resolve();
          } else {
            reject(new Error(result.message));
          }
        });
      });
    }
  }
  
  /**
   * 清空页面栈并跳转（clear + push）
   * @param url 目标页面路径
   * @param params 传递参数
   */
  static async clearAndPush(
    url: string, 
    params?: Record<string, Object>
  ): Promise<void> {
    PlatformLogger.info(`Clear and push to: ${url}`);
    
    if (isHarmonyOS()) {
      await this.initHarmonyRouter();
      
      // @ts-ignore
      if (this.harmonyRouter) {
        try {
          // @ts-ignore
          await this.harmonyRouter.clear();
          await this.push(url, params);
          PlatformLogger.info('Clear and push success');
        } catch (err) {
          PlatformLogger.error('Clear and push failed: ' + JSON.stringify(err));
          throw err;
        }
      }
    } else {
      return new Promise((resolve, reject) => {
        bridge.call({
          moduleName: 'Router',
          methodName: 'clearAndPush',
          params: { url, params }
        }, (result: BridgeResult) => {
          if (result.code === 0) {
            resolve();
          } else {
            reject(new Error(result.message));
          }
        });
      });
    }
  }
  
  /**
   * 获取当前页面参数
   */
  static getParams(): Record<string, Object> | undefined {
    if (isHarmonyOS()) {
      // @ts-ignore
      if (this.harmonyRouter) {
        // @ts-ignore
        return this.harmonyRouter.getParams();
      }
    } else {
      const result = bridge.callSync({
        moduleName: 'Router',
        methodName: 'getParams'
      });
      return result.data || undefined;
    }
    return undefined;
  }
  
  /**
   * 获取页面栈长度
   */
  static getLength(): number {
    if (isHarmonyOS()) {
      // @ts-ignore
      if (this.harmonyRouter) {
        // @ts-ignore
        return this.harmonyRouter.getLength();
      }
    } else {
      const result = bridge.callSync({
        moduleName: 'Router',
        methodName: 'getLength'
      });
      return result.data?.['length'] as number || 0;
    }
    return 0;
  }
  
  /**
   * 获取页面栈状态
   */
  static getState(): Object | undefined {
    if (isHarmonyOS()) {
      // @ts-ignore
      if (this.harmonyRouter) {
        // @ts-ignore
        return this.harmonyRouter.getState();
      }
    } else {
      const result = bridge.callSync({
        moduleName: 'Router',
        methodName: 'getState'
      });
      return result.data || undefined;
    }
    return undefined;
  }
}

// 导出便捷方法
export const router = CrossPlatformRouter;

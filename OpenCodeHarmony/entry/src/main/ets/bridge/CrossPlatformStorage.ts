/**
 * 跨平台数据存储封装
 * 
 * HarmonyOS: 使用 @ohos.data.preferences
 * Android: 使用 SharedPreferences
 * iOS: 使用 UserDefaults
 */

import { isHarmonyOS, PlatformLogger } from './Platform';
import { bridge, BridgeResult } from './Bridge';

// 存储值类型
export type StorageValue = string | number | boolean | Object | null;

/**
 * 跨平台数据存储类
 */
export class CrossPlatformStorage {
  private static TAG = 'CrossPlatformStorage';
  private static harmonyPreferences: Object | null = null;
  private static harmonyContext: Object | null = null;
  private static preferences: Object | null = null;
  private static defaultStoreName = 'default_store';
  
  /**
   * 初始化存储
   * @param storeName 存储名称
   * @param context HarmonyOS 上下文（仅 HarmonyOS 需要）
   */
  static async init(storeName?: string, context?: Object): Promise<void> {
    const name = storeName || this.defaultStoreName;
    PlatformLogger.info(`Initializing storage: ${name}`);
    
    if (isHarmonyOS()) {
      await this.initHarmonyStorage(name, context);
    } else {
      await this.initBridgeStorage(name);
    }
  }
  
  /**
   * 初始化 HarmonyOS 存储
   */
  private static async initHarmonyStorage(storeName: string, context?: Object): Promise<void> {
    if (!context) {
      PlatformLogger.warn('Context is required for HarmonyOS storage');
      return;
    }
    
    // @ts-ignore: 跨平台编译
    if (!this.harmonyPreferences) {
      // @ts-ignore
      this.harmonyPreferences = await import('@ohos.data.preferences');
    }
    
    this.harmonyContext = context;
    
    try {
      // @ts-ignore
      this.preferences = await this.harmonyPreferences.getPreferences(context, storeName);
      PlatformLogger.info('HarmonyOS storage initialized');
    } catch (err) {
      PlatformLogger.error('Failed to init HarmonyOS storage: ' + JSON.stringify(err));
      throw err;
    }
  }
  
  /**
   * 初始化 Bridge 存储
   */
  private static async initBridgeStorage(storeName: string): Promise<void> {
    return new Promise((resolve, reject) => {
      bridge.call({
        moduleName: 'Storage',
        methodName: 'init',
        params: { storeName }
      }, (result: BridgeResult) => {
        if (result.code === 0) {
          PlatformLogger.info('Bridge storage initialized');
          resolve();
        } else {
          PlatformLogger.error('Failed to init Bridge storage: ' + result.message);
          reject(new Error(result.message));
        }
      });
    });
  }
  
  /**
   * 存储数据
   * @param key 键名
   * @param value 值
   */
  static async put(key: string, value: StorageValue): Promise<void> {
    PlatformLogger.info(`Put: ${key}`);
    
    if (isHarmonyOS()) {
      return this.harmonyPut(key, value);
    } else {
      return this.bridgePut(key, value);
    }
  }
  
  /**
   * HarmonyOS 存储
   */
  private static async harmonyPut(key: string, value: StorageValue): Promise<void> {
    // @ts-ignore
    if (!this.preferences) {
      throw new Error('Storage not initialized');
    }
    
    try {
      // @ts-ignore
      await this.preferences.put(key, value);
      // @ts-ignore
      await this.preferences.flush();
      PlatformLogger.info(`Put success: ${key}`);
    } catch (err) {
      PlatformLogger.error(`Put failed: ${key}, ${JSON.stringify(err)}`);
      throw err;
    }
  }
  
  /**
   * Bridge 存储
   */
  private static async bridgePut(key: string, value: StorageValue): Promise<void> {
    return new Promise((resolve, reject) => {
      bridge.call({
        moduleName: 'Storage',
        methodName: 'put',
        params: { key, value }
      }, (result: BridgeResult) => {
        if (result.code === 0) {
          resolve();
        } else {
          reject(new Error(result.message));
        }
      });
    });
  }
  
  /**
   * 获取数据
   * @param key 键名
   * @param defaultValue 默认值
   */
  static async get<T extends StorageValue>(key: string, defaultValue?: T): Promise<T | undefined> {
    PlatformLogger.info(`Get: ${key}`);
    
    if (isHarmonyOS()) {
      return this.harmonyGet(key, defaultValue);
    } else {
      return this.bridgeGet(key, defaultValue);
    }
  }
  
  /**
   * HarmonyOS 获取
   */
  private static async harmonyGet<T extends StorageValue>(key: string, defaultValue?: T): Promise<T | undefined> {
    // @ts-ignore
    if (!this.preferences) {
      throw new Error('Storage not initialized');
    }
    
    try {
      // @ts-ignore
      const value = await this.preferences.get(key, defaultValue);
      PlatformLogger.info(`Get success: ${key}`);
      return value as T;
    } catch (err) {
      PlatformLogger.error(`Get failed: ${key}, ${JSON.stringify(err)}`);
      return defaultValue;
    }
  }
  
  /**
   * Bridge 获取
   */
  private static async bridgeGet<T extends StorageValue>(key: string, defaultValue?: T): Promise<T | undefined> {
    return new Promise((resolve) => {
      bridge.call({
        moduleName: 'Storage',
        methodName: 'get',
        params: { key, defaultValue }
      }, (result: BridgeResult) => {
        if (result.code === 0 && result.data) {
          resolve(result.data['value'] as T);
        } else {
          resolve(defaultValue);
        }
      });
    });
  }
  
  /**
   * 删除数据
   * @param key 键名
   */
  static async delete(key: string): Promise<void> {
    PlatformLogger.info(`Delete: ${key}`);
    
    if (isHarmonyOS()) {
      // @ts-ignore
      if (!this.preferences) {
        throw new Error('Storage not initialized');
      }
      
      try {
        // @ts-ignore
        await this.preferences.delete(key);
        // @ts-ignore
        await this.preferences.flush();
        PlatformLogger.info(`Delete success: ${key}`);
      } catch (err) {
        PlatformLogger.error(`Delete failed: ${key}, ${JSON.stringify(err)}`);
        throw err;
      }
    } else {
      return new Promise((resolve, reject) => {
        bridge.call({
          moduleName: 'Storage',
          methodName: 'delete',
          params: { key }
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
   * 检查是否存在
   * @param key 键名
   */
  static async has(key: string): Promise<boolean> {
    PlatformLogger.info(`Has: ${key}`);
    
    if (isHarmonyOS()) {
      // @ts-ignore
      if (!this.preferences) {
        throw new Error('Storage not initialized');
      }
      
      try {
        // @ts-ignore
        const exists = await this.preferences.has(key);
        return exists;
      } catch (err) {
        PlatformLogger.error(`Has failed: ${key}, ${JSON.stringify(err)}`);
        return false;
      }
    } else {
      return new Promise((resolve) => {
        bridge.call({
          moduleName: 'Storage',
          methodName: 'has',
          params: { key }
        }, (result: BridgeResult) => {
          if (result.code === 0 && result.data) {
            resolve(result.data['exists'] as boolean);
          } else {
            resolve(false);
          }
        });
      });
    }
  }
  
  /**
   * 获取所有键
   */
  static async getAllKeys(): Promise<string[]> {
    PlatformLogger.info('Get all keys');
    
    if (isHarmonyOS()) {
      // @ts-ignore
      if (!this.preferences) {
        throw new Error('Storage not initialized');
      }
      
      try {
        // @ts-ignore
        const keys = await this.preferences.getAll();
        return Object.keys(keys);
      } catch (err) {
        PlatformLogger.error('Get all keys failed: ' + JSON.stringify(err));
        return [];
      }
    } else {
      return new Promise((resolve) => {
        bridge.call({
          moduleName: 'Storage',
          methodName: 'getAllKeys'
        }, (result: BridgeResult) => {
          if (result.code === 0 && result.data) {
            resolve(result.data['keys'] as string[]);
          } else {
            resolve([]);
          }
        });
      });
    }
  }
  
  /**
   * 清空存储
   */
  static async clear(): Promise<void> {
    PlatformLogger.info('Clear storage');
    
    if (isHarmonyOS()) {
      // @ts-ignore
      if (!this.preferences) {
        throw new Error('Storage not initialized');
      }
      
      try {
        // @ts-ignore
        await this.preferences.clear();
        // @ts-ignore
        await this.preferences.flush();
        PlatformLogger.info('Clear success');
      } catch (err) {
        PlatformLogger.error('Clear failed: ' + JSON.stringify(err));
        throw err;
      }
    } else {
      return new Promise((resolve, reject) => {
        bridge.call({
          moduleName: 'Storage',
          methodName: 'clear'
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
   * 持久化存储（仅 HarmonyOS 需要）
   */
  static async flush(): Promise<void> {
    if (isHarmonyOS()) {
      // @ts-ignore
      if (!this.preferences) {
        throw new Error('Storage not initialized');
      }
      
      try {
        // @ts-ignore
        await this.preferences.flush();
        PlatformLogger.info('Flush success');
      } catch (err) {
        PlatformLogger.error('Flush failed: ' + JSON.stringify(err));
        throw err;
      }
    }
    // Android/iOS 自动持久化，无需额外操作
  }
}

// 导出便捷实例
export const storage = CrossPlatformStorage;

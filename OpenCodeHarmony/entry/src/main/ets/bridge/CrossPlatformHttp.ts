/**
 * 跨平台 HTTP 请求封装
 * 
 * HarmonyOS: 使用 @ohos.net.http
 * Android/iOS: 通过 Bridge 调用原生网络库
 */

import { isHarmonyOS, PlatformLogger } from './Platform';
import { bridge, BridgeResult } from './Bridge';

// HTTP 方法枚举
export enum HttpMethod {
  GET = 'GET',
  POST = 'POST',
  PUT = 'PUT',
  DELETE = 'DELETE',
  PATCH = 'PATCH',
  HEAD = 'HEAD',
  OPTIONS = 'OPTIONS'
}

// HTTP 请求配置
export interface HttpRequestConfig {
  url: string;
  method?: HttpMethod;
  headers?: Record<string, string>;
  params?: Record<string, string | number>;  // URL 参数
  data?: Object | string;  // 请求体
  timeout?: number;  // 超时时间（毫秒）
  expectDataType?: HttpDataType;  // 期望返回数据类型
}

// HTTP 响应
export interface HttpResponse {
  statusCode: number;
  headers: Record<string, string>;
  data: Object | string | null;
  error?: string;
}

// 数据类型
export enum HttpDataType {
  STRING = 'STRING',
  OBJECT = 'OBJECT',
  ARRAY_BUFFER = 'ARRAY_BUFFER'
}

/**
 * 跨平台 HTTP 请求类
 */
export class CrossPlatformHttp {
  private static TAG = 'CrossPlatformHttp';
  private static harmonyHttp: Object | null = null;
  private static defaultTimeout = 30000;  // 默认超时 30 秒
  
  /**
   * 初始化 HarmonyOS HTTP
   */
  private static async initHarmonyHttp(): Promise<void> {
    if (!isHarmonyOS()) return;
    
    if (!this.harmonyHttp) {
      // @ts-ignore: 跨平台编译
      this.harmonyHttp = await import('@ohos.net.http');
    }
  }
  
  /**
   * 创建 HTTP 请求
   */
  private static async createRequest(config: HttpRequestConfig): Promise<HttpResponse> {
    if (isHarmonyOS()) {
      return this.harmonyRequest(config);
    } else {
      return this.bridgeRequest(config);
    }
  }
  
  /**
   * HarmonyOS 原生 HTTP 请求
   */
  private static async harmonyRequest(config: HttpRequestConfig): Promise<HttpResponse> {
    await this.initHarmonyHttp();
    
    return new Promise((resolve, reject) => {
      // @ts-ignore
      if (!this.harmonyHttp) {
        reject(new Error('HTTP module not initialized'));
        return;
      }
      
      // @ts-ignore
      const httpRequest = this.harmonyHttp.createHttp();
      
      // 构建请求 URL
      let url = config.url;
      if (config.params) {
        const queryString = Object.entries(config.params)
          .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`)
          .join('&');
        url += (url.includes('?') ? '&' : '?') + queryString;
      }
      
      // 构建请求选项
      const options = {
        method: config.method || HttpMethod.GET,
        header: config.headers || {},
        extraData: config.data,
        connectTimeout: config.timeout || this.defaultTimeout,
        readTimeout: config.timeout || this.defaultTimeout,
        expectDataType: this.mapDataType(config.expectDataType)
      };
      
      PlatformLogger.info(`HTTP Request: ${options.method} ${url}`);
      
      // @ts-ignore
      httpRequest.request(url, options, (err: Object, data: Object) => {
        // @ts-ignore
        httpRequest.destroy();
        
        // @ts-ignore
        if (err) {
          PlatformLogger.error('HTTP Error: ' + JSON.stringify(err));
          resolve({
            statusCode: 0,
            headers: {},
            data: null,
            // @ts-ignore
            error: err.message || JSON.stringify(err)
          });
          return;
        }
        
        // @ts-ignore
        PlatformLogger.info(`HTTP Response: ${data?.responseCode}`);
        resolve({
          // @ts-ignore
          statusCode: data?.responseCode || 0,
          // @ts-ignore
          headers: data?.header || {},
          // @ts-ignore
          data: data?.result
        });
      });
    });
  }
  
  /**
   * Android/iOS Bridge HTTP 请求
   */
  private static async bridgeRequest(config: HttpRequestConfig): Promise<HttpResponse> {
    return new Promise((resolve) => {
      bridge.call({
        moduleName: 'Http',
        methodName: 'request',
        params: config
      }, (result: BridgeResult) => {
        if (result.code === 0 && result.data) {
          resolve({
            statusCode: result.data['statusCode'] as number || 0,
            headers: result.data['headers'] as Record<string, string> || {},
            data: result.data['data']
          });
        } else {
          resolve({
            statusCode: 0,
            headers: {},
            data: null,
            error: result.message
          });
        }
      });
    });
  }
  
  /**
   * 映射数据类型
   */
  private static mapDataType(type?: HttpDataType): Object {
    if (!type || !this.harmonyHttp) return {};
    
    // @ts-ignore
    const HttpDataType_ = this.harmonyHttp.HttpDataType;
    
    switch (type) {
      case HttpDataType.STRING:
        // @ts-ignore
        return HttpDataType_.STRING;
      case HttpDataType.OBJECT:
        // @ts-ignore
        return HttpDataType_.OBJECT;
      case HttpDataType.ARRAY_BUFFER:
        // @ts-ignore
        return HttpDataType_.ARRAY_BUFFER;
      default:
        return {};
    }
  }
  
  // ==================== 便捷方法 ====================
  
  /**
   * GET 请求
   */
  static async get(
    url: string, 
    params?: Record<string, string | number>,
    headers?: Record<string, string>
  ): Promise<HttpResponse> {
    return this.createRequest({
      url,
      method: HttpMethod.GET,
      params,
      headers
    });
  }
  
  /**
   * POST 请求
   */
  static async post(
    url: string, 
    data?: Object | string,
    headers?: Record<string, string>
  ): Promise<HttpResponse> {
    return this.createRequest({
      url,
      method: HttpMethod.POST,
      data,
      headers
    });
  }
  
  /**
   * PUT 请求
   */
  static async put(
    url: string, 
    data?: Object | string,
    headers?: Record<string, string>
  ): Promise<HttpResponse> {
    return this.createRequest({
      url,
      method: HttpMethod.PUT,
      data,
      headers
    });
  }
  
  /**
   * DELETE 请求
   */
  static async delete(
    url: string, 
    data?: Object | string,
    headers?: Record<string, string>
  ): Promise<HttpResponse> {
    return this.createRequest({
      url,
      method: HttpMethod.DELETE,
      data,
      headers
    });
  }
  
  /**
   * PATCH 请求
   */
  static async patch(
    url: string, 
    data?: Object | string,
    headers?: Record<string, string>
  ): Promise<HttpResponse> {
    return this.createRequest({
      url,
      method: HttpMethod.PATCH,
      data,
      headers
    });
  }
  
  /**
   * 通用请求方法
   */
  static async request(config: HttpRequestConfig): Promise<HttpResponse> {
    return this.createRequest(config);
  }
  
  /**
   * 设置默认超时时间
   */
  static setDefaultTimeout(timeout: number): void {
    this.defaultTimeout = timeout;
  }
}

// 导出便捷实例
export const http = CrossPlatformHttp;

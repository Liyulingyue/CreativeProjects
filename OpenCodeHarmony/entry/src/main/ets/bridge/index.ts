/**
 * 跨平台桥接层导出入口
 * 
 * 使用说明：
 * 1. 导入平台检测：import { isHarmonyOS, getPlatform } from '../bridge'
 * 2. 导入 Bridge：import { bridge, Bridge } from '../bridge'
 * 3. 导入跨平台 API：import { CrossPlatformRouter, CrossPlatformHttp } from '../bridge'
 */

// 平台检测
export { 
  PlatformType, 
  getPlatform, 
  isHarmonyOS, 
  isAndroid, 
  isIOS,
  PlatformLogger 
} from './Platform';

// Bridge 桥接
export { 
  Bridge, 
  BridgeResult, 
  BridgeConfig, 
  BridgeCallback,
  bridge 
} from './Bridge';

// 跨平台 API（后续添加）
export { CrossPlatformRouter } from './CrossPlatformRouter';
export { CrossPlatformHttp, HttpMethod, HttpResponse } from './CrossPlatformHttp';
export { CrossPlatformStorage } from './CrossPlatformStorage';
export { CrossPlatformPrompt } from './CrossPlatformPrompt';

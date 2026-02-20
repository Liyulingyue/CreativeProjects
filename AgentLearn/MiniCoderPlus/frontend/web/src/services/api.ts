// API服务层 - 与MiniCoder后端交互

import { CodeRequest, ApiResponse } from '../types';

const API_BASE_URL = process.env.REACT_APP_API_URL || 'http://localhost:8000';

export class MiniCoderApi {
  /**
   * 生成代码
   */
  static async generateCode(prompt: string, language: string = 'python'): Promise<ApiResponse> {
    try {
      const response = await fetch(`${API_BASE_URL}/api/generate`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ prompt, language }),
      });
      
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      
      const data = await response.json();
      return { success: true, data };
    } catch (error) {
      console.error('生成代码失败:', error);
      return { 
        success: false, 
        error: error instanceof Error ? error.message : '未知错误'
      };
    }
  }

  /**
   * 解释代码
   */
  static async explainCode(code: string): Promise<ApiResponse> {
    try {
      const response = await fetch(`${API_BASE_URL}/api/explain`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ code }),
      });
      
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      
      const data = await response.json();
      return { success: true, data };
    } catch (error) {
      console.error('解释代码失败:', error);
      return { 
        success: false, 
        error: error instanceof Error ? error.message : '未知错误'
      };
    }
  }

  /**
   * 修复bug
   */
  static async fixBug(errorMessage: string, codeContext: string): Promise<ApiResponse> {
    try {
      const response = await fetch(`${API_BASE_URL}/api/fix`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ errorMessage, codeContext }),
      });
      
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      
      const data = await response.json();
      return { success: true, data };
    } catch (error) {
      console.error('修复bug失败:', error);
      return { 
        success: false, 
        error: error instanceof Error ? error.message : '未知错误'
      };
    }
  }

  /**
   * 优化代码
   */
  static async optimizeCode(code: string): Promise<ApiResponse> {
    try {
      const response = await fetch(`${API_BASE_URL}/api/optimize`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ code }),
      });
      
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      
      const data = await response.json();
      return { success: true, data };
    } catch (error) {
      console.error('优化代码失败:', error);
      return { 
        success: false, 
        error: error instanceof Error ? error.message : '未知错误'
      };
    }
  }
}
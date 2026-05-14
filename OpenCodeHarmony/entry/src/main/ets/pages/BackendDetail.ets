import { OpenCodeCore, OpenCodeBackend } from '../core/OpenCodeCore';
import router from '@ohos.router';
import http from '@ohos.net.http';

// 健康检查响应接口
interface HealthResponse {
  healthy: boolean;
}

@Entry
@Component
struct BackendDetail {
  @State url: string = 'http://';
  @State authToken: string = '';
  @State notes: string = '';
  @State isEdit: boolean = false;
  @State editId: string = '';
  @State isTesting: boolean = false;
  @State testResult: string = ''; // 'success' | 'fail' | ''
  private core: OpenCodeCore = OpenCodeCore.getInstance();

  aboutToAppear() {
    // 获取路由参数，支持编辑模式
    const params = router.getParams() as Record<string, Object>;
    if (params && params['id']) {
      this.isEdit = true;
      this.editId = params['id'] as string;
      this.url = (params['url'] as string) || 'http://';
      this.authToken = (params['authToken'] as string) || '';
      this.notes = (params['notes'] as string) || '';
    }
  }

  // 测试连通性
  testConnection() {
    if (!this.url || this.url === 'http://') {
      this.testResult = 'fail';
      return;
    }
    this.isTesting = true;
    this.testResult = '';
    const normalizedUrl = this.url.endsWith('/') ? this.url.slice(0, -1) : this.url;
    const request = http.createHttp();
    const headers: Record<string, string> = {};
    if (this.authToken) {
      headers['Authorization'] = 'Basic ' + this.base64Encode('opencode:' + this.authToken);
    }
    request.request(
      normalizedUrl + '/global/health',
      {
        method: http.RequestMethod.GET,
        header: headers,
        connectTimeout: 5000,
        readTimeout: 5000,
      },
      (err, data) => {
        this.isTesting = false;
        if (err || data.responseCode !== 200) {
          this.testResult = 'fail';
        } else {
          try {
            const json: HealthResponse = JSON.parse(data.result as string) as HealthResponse;
            this.testResult = json.healthy === true ? 'success' : 'fail';
          } catch {
            this.testResult = 'fail';
          }
        }
        request.destroy();
      }
    );
  }

  // Base64 encode for Basic auth
  private base64Encode(str: string): string {
    const chars: string[] = [
      'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M',
      'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
      'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm',
      'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
      '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '+', '/'
    ];
    let result = '';
    const bytes = new Uint8Array(str.length);
    for (let i = 0; i < str.length; i++) {
      bytes[i] = str.charCodeAt(i);
    }
    let i = 0;
    while (i < bytes.length) {
      const b1 = bytes[i++];
      const b2 = i < bytes.length ? bytes[i++] : NaN;
      const b3 = i < bytes.length ? bytes[i++] : NaN;
      result += chars[b1 >> 2];
      result += chars[((b1 & 0x03) << 4) | (isNaN(b2) ? 0 : b2 >> 4)];
      result += isNaN(b2) ? '=' : chars[((b2 & 0x0f) << 2) | (isNaN(b3) ? 0 : b3 >> 6)];
      result += isNaN(b3) ? '=' : chars[b3 & 0x3f];
    }
    return result;
  }

  // 保存后端配置
  async saveBackend() {
    if (!this.url || this.url === 'http://') {
      return;
    }
    const normalizedUrl = this.url.endsWith('/') ? this.url.slice(0, -1) : this.url;
    if (this.isEdit) {
      await this.core.updateBackend(this.editId, normalizedUrl, this.authToken, this.notes);
    } else {
      await this.core.addBackend(normalizedUrl, this.authToken, this.notes);
    }
    // 触发后端列表刷新
    AppStorage.SetOrCreate<number>('backendRefreshTag', Date.now());
    router.back();
  }

  build() {
    Column() {
      // 顶部导航栏
      Row() {
        Text('←')
          .fontSize(24)
          .fontColor('#333333')
          .padding({ right: 16 })
          .onClick(() => router.back())

        Text(this.isEdit ? '编辑后端' : '新增后端')
          .fontSize(18)
          .fontWeight(FontWeight.Bold)
          .layoutWeight(1)
      }
      .width('100%')
      .height(56)
      .padding({ left: 16, right: 16 })
      .backgroundColor('#FFFFFF')

      Divider().height(0.5).color('#E0E0E0')

      // 表单内容
      Column() {
        // 后端地址
        Column() {
          Text('后端地址')
            .fontSize(13)
            .fontColor('#999999')
            .margin({ bottom: 8 })
          TextInput({ placeholder: 'http://192.168.1.5:3000', text: this.url })
            .onChange(v => this.url = v)
            .height(44)
            .backgroundColor('#F5F5F5')
            .borderRadius(8)
        }
        .width('100%')
        .alignItems(HorizontalAlign.Start)
        .margin({ bottom: 20 })

        // 鉴权密钥
        Column() {
          Text('鉴权密钥 / Token')
            .fontSize(13)
            .fontColor('#999999')
            .margin({ bottom: 8 })
          TextInput({ placeholder: '可选，用于身份验证', text: this.authToken })
            .type(InputType.Password)
            .onChange(v => this.authToken = v)
            .height(44)
            .backgroundColor('#F5F5F5')
            .borderRadius(8)
        }
        .width('100%')
        .alignItems(HorizontalAlign.Start)
        .margin({ bottom: 20 })

        // 备注
        Column() {
          Text('备注')
            .fontSize(13)
            .fontColor('#999999')
            .margin({ bottom: 8 })
          TextInput({ placeholder: '可选，方便识别', text: this.notes })
            .onChange(v => this.notes = v)
            .height(44)
            .backgroundColor('#F5F5F5')
            .borderRadius(8)
        }
        .width('100%')
        .alignItems(HorizontalAlign.Start)
        .margin({ bottom: 32 })

        // 测试连通按钮
        Button({ type: ButtonType.Normal }) {
          Row({ space: 8 }) {
            if (this.isTesting) {
              LoadingProgress()
                .width(16)
                .height(16)
                .color('#FFFFFF')
            }
            Text(this.isTesting ? '测试中...' : '测试连通')
              .fontSize(15)
          }
        }
        .width('100%')
        .height(44)
        .borderRadius(8)
        .backgroundColor('#1677FF')
        .fontColor('#FFFFFF')
        .enabled(!this.isTesting)
        .onClick(() => this.testConnection())

        // 测试结果
        if (this.testResult !== '') {
          Row({ space: 8 }) {
            Text(this.testResult === 'success' ? '✓' : '✗')
              .fontSize(16)
              .fontColor(this.testResult === 'success' ? '#07C160' : '#FF4D4F')
            Text(this.testResult === 'success' ? '连接成功' : '连接失败')
              .fontSize(14)
              .fontColor(this.testResult === 'success' ? '#07C160' : '#FF4D4F')
          }
          .margin({ top: 16 })
        }
      }
      .width('100%')
      .padding({ left: 20, right: 20, top: 24 })
      .layoutWeight(1)

      // 底部按钮
      Column({ space: 12 }) {
        if (this.isEdit) {
          Button('删除此后端', { type: ButtonType.Normal })
            .width('100%')
            .height(44)
            .borderRadius(8)
            .backgroundColor('#FFF2F0')
            .fontColor('#FF4D4F')
            .onClick(async () => {
              await this.core.removeBackend(this.editId);
              AppStorage.SetOrCreate<number>('backendRefreshTag', Date.now());
              router.back();
            })
        }

        Row({ space: 12 }) {
          Button('取消', { type: ButtonType.Normal })
            .layoutWeight(1)
            .height(44)
            .borderRadius(8)
            .backgroundColor('#F0F0F0')
            .fontColor('#333333')
            .onClick(() => router.back())

          Button('确认', { type: ButtonType.Normal })
            .layoutWeight(1)
            .height(44)
            .borderRadius(8)
            .backgroundColor('#07C160')
            .fontColor('#FFFFFF')
            .enabled(this.url.length > 10)
            .onClick(() => this.saveBackend())
        }
        .width('100%')
      }
      .width('100%')
      .padding({ left: 20, right: 20, bottom: 32 })
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#FFFFFF')
  }
}

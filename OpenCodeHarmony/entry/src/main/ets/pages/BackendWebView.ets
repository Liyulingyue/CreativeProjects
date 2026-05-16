import router from '@ohos.router';
import webview from '@ohos.web.webview';
import url from '@ohos.url';

@Entry
@Component
struct BackendWebView {
  @State backendUrl: string = '';
  @State username: string = '';
  @State authToken: string = '';
  @State title: string = 'OpenCode';
  controller: webview.WebviewController = new webview.WebviewController();

  aboutToAppear() {
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.backendUrl = (params['url'] as string) || '';
      this.username = (params['username'] as string) || 'opencode';
      this.authToken = (params['authToken'] as string) || '';
      this.title = (params['notes'] as string) || 'OpenCode';
    }
  }

  private buildAuthUrl(): string {
    if (!this.authToken) {
      return this.backendUrl;
    }
    let urlStr = this.backendUrl;
    if (!urlStr.includes('://')) {
      urlStr = 'http://' + urlStr;
    }
    const baseUrl = new url.URL(urlStr);
    baseUrl.searchParams.set('auth_token', this.base64Encode(this.username + ':' + this.authToken));
    return baseUrl.toString();
  }

  private base64Encode(str: string): string {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    let result = '';
    let i = 0;
    const len = str.length;
    while (i < len) {
      const b1 = str.charCodeAt(i);
      i++;
      const b2 = i < len ? str.charCodeAt(i) : NaN;
      if (i < len) i++;
      const b3 = i < len ? str.charCodeAt(i) : NaN;
      if (i < len) i++;
      result += chars.charAt(b1 >> 2);
      result += chars.charAt(((b1 & 0x03) << 4) | (isNaN(b2) ? 0 : b2 >> 4));
      result += isNaN(b2) ? '=' : chars.charAt(((b2 & 0x0f) << 2) | (isNaN(b3) ? 0 : b3 >> 6));
      result += isNaN(b3) ? '=' : chars.charAt(b3 & 0x3f);
    }
    return result;
  }

  build() {
    Column() {
      Row() {
        Text('<')
          .fontSize(24)
          .fontColor('#333333')
          .padding({ right: 16 })
          .onClick(() => router.back())

        Text(this.title)
          .fontSize(18)
          .fontWeight(FontWeight.Bold)
          .layoutWeight(1)
          .maxLines(1)
      }
      .width('100%')
      .height(56)
      .padding({ left: 16, right: 16 })
      .backgroundColor('#FFFFFF')

      Divider().height(0.5).color('#E0E0E0')

      Web({ src: this.buildAuthUrl(), controller: this.controller })
        .width('100%')
        .layoutWeight(1)
        .onPageBegin(() => {
          const ua = this.controller.getCustomUserAgent();
          if (!ua.includes('opencode-harmony/1.0')) {
            this.controller.setCustomUserAgent(ua + ' opencode-harmony/1.0');
          }
        })
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#FFFFFF')
  }

}

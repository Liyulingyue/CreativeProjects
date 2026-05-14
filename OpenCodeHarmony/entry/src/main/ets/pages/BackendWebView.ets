import router from '@ohos.router';
import webview from '@ohos.web.webview';

@Entry
@Component
struct BackendWebView {
  @State backendUrl: string = '';
  @State authToken: string = '';
  @State title: string = 'OpenCode';
  controller: webview.WebviewController = new webview.WebviewController();

  aboutToAppear() {
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.backendUrl = (params['url'] as string) || '';
      this.authToken = (params['authToken'] as string) || '';
      this.title = (params['notes'] as string) || 'OpenCode';
    }
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

      Web({ src: this.backendUrl, controller: this.controller })
        .width('100%')
        .layoutWeight(1)
        .onPageBegin(() => {
          if (this.authToken) {
            this.controller.setCustomUserAgent(
              this.controller.getCustomUserAgent() + ' opencode-harmony/1.0'
            );
          }
        })
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#FFFFFF')
  }

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
}

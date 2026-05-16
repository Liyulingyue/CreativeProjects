import router from '@ohos.router';
import webview from '@ohos.web.webview';

@Entry
@Component
struct BackendWebView {
  @State backendUrl: string = '';
  @State title: string = 'OpenCode';
  controller: webview.WebviewController = new webview.WebviewController();

  aboutToAppear() {
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.backendUrl = (params['url'] as string) || '';
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
          this.controller.setCustomUserAgent(
            this.controller.getCustomUserAgent() + ' opencode-harmony/1.0'
          );
        })
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#FFFFFF')
  }

}

import router from '@ohos.router';
import webview from '@ohos.web.webview';
import url from '@ohos.url';
import http from '@ohos.net.http';

interface PageBeginEvent {
  url?: string;
}

interface PageEndEvent {
  url?: string;
}

@Entry
@Component
struct BackendWebView {
  @State backendUrl: string = '';
  @State username: string = '';
  @State authToken: string = '';
  @State title: string = 'OpenCode';
  @State currentUrl: string = '';
  @StorageLink('themeBgCard') themeBgCard: string = '#FFFFFF';
  @StorageLink('themeTextPrimary') themeTextPrimary: string = '#333333';
  @StorageLink('themeBorderPrimary') themeBorderPrimary: string = '#E0E0E0';
  controller: webview.WebviewController = new webview.WebviewController();
  private storageShimInjectedForUrl: string = '';

  aboutToAppear() {
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.backendUrl = (params['url'] as string) || '';
      this.username = (params['username'] as string) || 'opencode';
      this.authToken = (params['authToken'] as string) || '';
      this.title = (params['notes'] as string) || 'OpenCode';
    }
    this.currentUrl = this.normalizeUrl(this.backendUrl);
  }

  private normalizeUrl(rawUrl: string): string {
    if (!rawUrl) {
      return rawUrl;
    }
    let urlStr = rawUrl;
    if (!urlStr.includes('://')) {
      urlStr = 'http://' + urlStr;
    }
    return urlStr;
  }

  private injectStorageShim(pageUrl: string) {
    if (!pageUrl || this.storageShimInjectedForUrl === pageUrl) {
      return;
    }
    this.storageShimInjectedForUrl = pageUrl;
    this.controller.runJavaScript(
      `(function () {
        function createMemoryStorage() {
          var store = {};
          return {
            getItem: function (key) {
              key = String(key);
              return Object.prototype.hasOwnProperty.call(store, key) ? store[key] : null;
            },
            setItem: function (key, value) {
              store[String(key)] = String(value);
            },
            removeItem: function (key) {
              delete store[String(key)];
            },
            clear: function () {
              store = {};
            },
            key: function (index) {
              var keys = Object.keys(store);
              return index >= 0 && index < keys.length ? keys[index] : null;
            }
          };
        }
        function ensureStorage(name) {
          var current = globalThis[name];
          if (current && typeof current.getItem === 'function') {
            return;
          }
          var fallback = createMemoryStorage();
          try {
            Object.defineProperty(globalThis, name, {
              value: fallback,
              configurable: true,
              writable: true
            });
          } catch (e) {
            globalThis[name] = fallback;
          }
        }
        ensureStorage('localStorage');
        ensureStorage('sessionStorage');
      })();`
    );
  }

  build() {
    Column() {
      Row() {
        Text('<')
          .fontSize(24)
          .fontColor(this.themeTextPrimary)
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
      .backgroundColor(this.themeBgCard)

      Divider().height(0.5).color(this.themeBorderPrimary)

      Web({ src: this.currentUrl, controller: this.controller })
        .width('100%')
        .layoutWeight(1)
        .javaScriptAccess(true)
        .domStorageAccess(true)
        .onPageBegin(() => {
          const ua = this.controller.getCustomUserAgent();
          if (!ua.includes('opencode-harmony/1.0')) {
            this.controller.setCustomUserAgent(ua + ' opencode-harmony/1.0');
          }
        })
        .onPageEnd((event?: PageEndEvent) => {
          this.injectStorageShim(event?.url || this.currentUrl);
        })
        .onHttpAuthRequest((event) => {
          if (event) {
            // 如果 App 配置中已经提供了用户名和密码，则自动填充
            if (this.username && this.authToken) {
              event.handler.confirm(this.username, this.authToken);
              return true;
            }
          }
          // 返回 false，Web 组件会弹出系统原生的 Basic Auth 输入框（即手机浏览器那种弹框）
          return false;
        })
    }
    .width('100%')
    .height('100%')
    .backgroundColor(this.themeBgCard)
  }

}

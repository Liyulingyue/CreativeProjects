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
  @State bootHtml: string = '';
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
    this.currentUrl = this.appendAuthToken(this.backendUrl);
    this.loadBootHtml();
  }

  private appendAuthToken(rawUrl: string): string {
    if (!rawUrl) {
      return rawUrl;
    }
    let urlStr = rawUrl;
    if (!urlStr.includes('://')) {
      urlStr = 'http://' + urlStr;
    }
    if (!this.authToken) {
      return urlStr;
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

  private async loadBootHtml() {
    if (!this.currentUrl) {
      return;
    }
    const request = http.createHttp();
    const headers: Record<string, string> = {};
    if (this.authToken) {
      headers['Authorization'] = 'Basic ' + this.base64Encode(this.username + ':' + this.authToken);
    }
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        request.request(
          this.currentUrl,
          {
            method: http.RequestMethod.GET,
            header: headers,
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) {
              reject(err);
              return;
            }
            resolve(data);
          }
        );
      });
      if (result.responseCode !== 200) {
        return;
      }
      const html = result.result as string;
      const shim = '<script>(function(){function createMemoryStorage(){var store={};return{getItem:function(key){key=String(key);return Object.prototype.hasOwnProperty.call(store,key)?store[key]:null;},setItem:function(key,value){store[String(key)]=String(value);},removeItem:function(key){delete store[String(key)];},clear:function(){store={};},key:function(index){var keys=Object.keys(store);return index>=0&&index<keys.length?keys[index]:null;}};}function ensureStorage(name){var current=globalThis[name];if(current&&typeof current.getItem===\'function\'){return;}var fallback=createMemoryStorage();try{Object.defineProperty(globalThis,name,{value:fallback,configurable:true,writable:true});}catch(e){globalThis[name]=fallback;}}ensureStorage(\'localStorage\');ensureStorage(\'sessionStorage\');})();</script>';
      const headIndex = html.indexOf('<head>');
      if (headIndex >= 0) {
        this.bootHtml = html.slice(0, headIndex + 6) + shim + html.slice(headIndex + 6);
        return;
      }
      this.bootHtml = shim + html;
    } catch (e) {
      console.error('[BackendWebView] loadBootHtml error:', e);
    } finally {
      request.destroy();
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

      if (this.bootHtml) {
        Web({ src: this.currentUrl, controller: this.controller })
          .width('100%')
          .layoutWeight(1)
          .javaScriptAccess(true)
          .onControllerAttached(() => {
            this.controller.loadData(this.bootHtml, 'text/html', 'UTF-8');
          })
      } else {
        Web({ src: this.currentUrl, controller: this.controller })
        .width('100%')
        .layoutWeight(1)
        .onPageBegin((event?: PageBeginEvent) => {
          const ua = this.controller.getCustomUserAgent();
          if (!ua.includes('opencode-harmony/1.0')) {
            this.controller.setCustomUserAgent(ua + ' opencode-harmony/1.0');
          }
          const nextUrl = this.appendAuthToken(event?.url || this.currentUrl || this.backendUrl);
          if (nextUrl !== (event?.url || this.currentUrl)) {
            this.currentUrl = nextUrl;
            this.controller.loadUrl(nextUrl);
          }
        })
        .onPageEnd((event?: PageEndEvent) => {
          this.injectStorageShim(event?.url || this.currentUrl);
        })
      }
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#FFFFFF')
  }

}

import { OpenCodeCore } from '../core/OpenCodeCore';
import http from '@ohos.net.http';
import router from '@ohos.router';
import promptAction from '@ohos.promptAction';

interface UserInfo {
  id: string;
  username: string;
  plan: string;
  created_at: string;
}

interface TokenData {
  access_token: string;
}

interface RegisterErrorBody {
  detail?: string;
}

interface UpdateTimestampResp {
  timestamp: string;
  updated_at: string;
}

@Component
export struct AboutPage {
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  @State isImmersiveMode: boolean = true;
  @State messageLoadMode: string = 'all';
  @StorageLink('aboutPanelVisible') showAbout: boolean = false;
  @StorageLink('settingsPanelVisible') showSettings: boolean = false;
  @StorageLink('accountPanelVisible') showAccount: boolean = false;
  @State displayUsername: string = '';
  @State isRotationLocked: boolean = false;
  @State currentThemeName: string = '极光绿';
  @State isDarkMode: boolean = false;
  @StorageLink('themeAccent') themeAccent: string = '#07C160';
  @StorageLink('themeBgPrimary') themeBgPrimary: string = '#F5F5F5';
  @StorageLink('themeBgCard') themeBgCard: string = '#FFFFFF';
  @StorageLink('themeTextPrimary') themeTextPrimary: string = '#333333';
  @StorageLink('themeTextSecondary') themeTextSecondary: string = '#888888';
  @StorageLink('themeTextTertiary') themeTextTertiary: string = '#999999';
  @StorageLink('themeTextMuted') themeTextMuted: string = '#BBBBBB';
  @StorageLink('themeBorderPrimary') themeBorderPrimary: string = '#E0E0E0';
  @StorageLink('themeBorderSecondary') themeBorderSecondary: string = '#F0F0F0';
  @StorageLink('themeLink') themeLink: string = '#007DFF';
  @StorageLink('themeConfig') themeConfig: string = '#1677FF';
  @StorageLink('themeDivider') themeDivider: string = '#EEEEEE';
  @StorageLink('themeStatusWorking') themeStatusWorking: string = '#2196F3';
  @StorageLink('themeStatusUnread') themeStatusUnread: string = '#FF3B30';
  @StorageLink('themeBgSecondary') themeBgSecondary: string = '#EDEDED';
  @StorageLink('enableMarkdownRender') enableMarkdownRender: boolean = true;

  @State accountServerUrl: string = '';
  @State accountToken: string = '';
  @State isLoggedIn: boolean = false;
  @State currentUser: UserInfo | null = null;
  @State isLoading: boolean = false;
  @State errorMsg: string = '';
  @State loginUsername: string = '';
  @State loginPassword: string = '';
  @State registerUsername: string = '';
  @State registerPassword: string = '';
  @State registerConfirm: string = '';
  @State registerError: string = '';
  @State activeForm: 'login' | 'register' = 'login';
  @State showServerConfig: boolean = false;
  @State configServerUrl: string = '';
  @State syncLoading: boolean = false;
  @State syncMsg: string = '';
  @State redeemCode: string = '';
  @State redeemMsg: string = '';
  @State showUpdate: boolean = false;
  @State updateCheckLoading: boolean = false;
  private updateCheckCompletedCount: number = 0;
  private updateSuccessUrls: string[] = [];
  @State updateStatus: string = '';
  @State updateAvailable: boolean = false;
  @State localVersion: string = '0.1.0';
  @State remoteVersion: string = '';

  aboutToAppear() {
    this.isImmersiveMode = this.core.getImmersiveMode();
    this.displayUsername = this.core.getDisplayUsername();
    this.isRotationLocked = this.core.isRotationLocked();
    this.currentThemeName = this.core.getThemeName();
    this.isDarkMode = this.core.getDarkMode();
    this.messageLoadMode = this.core.getMessageLoadMode();
    AppStorage.setOrCreate<boolean>('rotationLocked', this.isRotationLocked);
    AppStorage.setOrCreate<boolean>('aboutPanelVisible', false);
    AppStorage.setOrCreate<boolean>('settingsPanelVisible', false);
    AppStorage.setOrCreate<boolean>('accountPanelVisible', false);
    this.loadAccountInfo();
  }

  private openUrl(url: string): void {
    console.info('[AboutPage] openUrl:', url);
  }

  private toggleMode() {
    this.isImmersiveMode = !this.isImmersiveMode;
    AppStorage.setOrCreate<boolean>('immersiveMode', this.isImmersiveMode);
  }

  private loadAccountInfo(): void {
    this.accountServerUrl = this.core.getAccountServerUrl();
    this.accountToken = this.core.getAccountToken();
    if (this.accountToken) {
      this.loadCurrentUser();
    }
  }

  private async fetchMe(): Promise<UserInfo | null> {
    if (!this.accountServerUrl || !this.accountToken) return null;

    const url = `${this.accountServerUrl.replace(/\/+$/, '')}/api/auth/me`;
    return new Promise((resolve) => {
      const req = http.createHttp();
      req.request(
        url,
        {
          method: http.RequestMethod.GET,
          header: { 'Authorization': 'Bearer ' + this.accountToken },
          connectTimeout: 10000,
          readTimeout: 10000
        },
        (err, data) => {
          req.destroy();
          if (err || data.responseCode !== 200) {
            resolve(null);
            return;
          }
          try {
            resolve(JSON.parse(data.result as string) as UserInfo);
          } catch {
            resolve(null);
          }
        }
      );
    });
  }

  private async doLogin(): Promise<void> {
    if (!this.loginUsername || !this.loginPassword) {
      this.errorMsg = '请填写用户名和密码';
      return;
    }
    if (!this.accountServerUrl) {
      this.errorMsg = '请先配置账户服务器地址';
      return;
    }
    this.isLoading = true;
    this.errorMsg = '';

    const url = `${this.accountServerUrl.replace(/\/+$/, '')}/api/auth/login`;
    const bodyStr = `username=${encodeURIComponent(this.loginUsername)}&password=${encodeURIComponent(this.loginPassword)}`;

    const req = http.createHttp();
    req.request(
      url,
      {
        method: http.RequestMethod.POST,
        header: { 'Content-Type': 'application/x-www-form-urlencoded' },
        extraData: bodyStr,
        connectTimeout: 10000,
        readTimeout: 10000
      },
      (err, data) => {
        req.destroy();
        this.isLoading = false;
        if (err) {
          this.errorMsg = '网络错误: ' + String(err);
          return;
        }
        if (data.responseCode === 200) {
          try {
            const tokenData = JSON.parse(data.result as string) as TokenData;
            this.core.setAccountToken(tokenData.access_token);
            this.accountToken = tokenData.access_token;
            this.isLoggedIn = true;
            this.loadCurrentUser();
          } catch {
            this.errorMsg = '响应解析失败';
          }
        } else if (data.responseCode === 401) {
          this.errorMsg = '用户名或密码错误';
        } else {
          this.errorMsg = `登录失败 (${data.responseCode})`;
        }
      }
    );
  }

  private async doRegister(): Promise<void> {
    if (!this.registerUsername || !this.registerPassword) {
      this.registerError = '请填写所有字段';
      return;
    }
    if (this.registerPassword !== this.registerConfirm) {
      this.registerError = '两次密码不一致';
      return;
    }
    if (this.registerPassword.length < 6) {
      this.registerError = '密码至少6位';
      return;
    }
    if (!this.accountServerUrl) {
      this.registerError = '请先配置账户服务器地址';
      return;
    }
    this.isLoading = true;
    this.registerError = '';

    const url = `${this.accountServerUrl.replace(/\/+$/, '')}/api/auth/register`;
    const req = http.createHttp();
    req.request(
      url,
      {
        method: http.RequestMethod.POST,
        header: { 'Content-Type': 'application/json' },
        extraData: JSON.stringify({ username: this.registerUsername, password: this.registerPassword }),
        connectTimeout: 10000,
        readTimeout: 10000
      },
      (err, data) => {
        req.destroy();
        this.isLoading = false;
        if (err) {
          this.registerError = '网络错误: ' + String(err);
          return;
        }
        if (data.responseCode === 200) {
          this.activeForm = 'login';
          this.loginUsername = this.registerUsername;
          this.loginPassword = '';
          this.registerUsername = '';
          this.registerPassword = '';
          this.registerConfirm = '';
          this.registerError = '';
        } else if (data.responseCode === 400) {
          try {
            const body = JSON.parse(data.result as string) as RegisterErrorBody;
            this.registerError = body.detail || '注册失败';
          } catch {
            this.registerError = '用户名已存在';
          }
        } else {
          this.registerError = `注册失败 (${data.responseCode})`;
        }
      }
    );
  }

  private async loadCurrentUser(): Promise<void> {
    const user = await this.fetchMe();
    if (user) {
      this.currentUser = user;
      this.isLoggedIn = true;
    }
  }

  private async logout(): Promise<void> {
    this.isLoggedIn = false;
    this.currentUser = null;
    this.loginUsername = '';
    this.loginPassword = '';
    await this.core.setAccountToken('');
    this.accountToken = '';
  }

  private async saveServerConfig(): Promise<void> {
    await this.core.setAccountServerUrl(this.configServerUrl.trim());
    this.accountServerUrl = this.configServerUrl.trim();
    this.showServerConfig = false;
    this.isLoggedIn = false;
    this.currentUser = null;
    this.accountToken = '';
    await this.core.setAccountToken('');
  }

  private async doSaveConfig(): Promise<void> {
    this.syncLoading = true;
    this.syncMsg = '';
    const result = await this.core.saveCurrentConfigToServer();
    this.syncLoading = false;
    if (result.success) {
      this.syncMsg = '保存成功';
    } else {
      this.syncMsg = result.error || '保存失败';
    }
    setTimeout(() => { this.syncMsg = ''; }, 3000);
  }

  private async doSyncConfig(): Promise<void> {
    this.syncLoading = true;
    this.syncMsg = '';
    const result = await this.core.syncConfigFromServer();
    this.syncLoading = false;
    if (result.success) {
      this.syncMsg = `同步成功：${result.backends} 个后端，${result.sessions} 个会话`;
    } else {
      this.syncMsg = result.error || '同步失败';
    }
    setTimeout(() => { this.syncMsg = ''; }, 3000);
  }

  private async doRedeem(): Promise<void> {
    if (!this.redeemCode.trim()) {
      this.redeemMsg = '请输入兑换码';
      return;
    }
    this.syncLoading = true;
    this.redeemMsg = '';
    const result = await this.core.redeemCode(this.redeemCode.trim().toUpperCase());
    this.syncLoading = false;
    if (result.success) {
      this.redeemMsg = result.message;
      this.redeemCode = '';
      this.loadCurrentUser();
      setTimeout(() => { this.redeemMsg = ''; }, 3000);
    } else {
      this.redeemMsg = result.message;
    }
  }

  @Builder
  ListItem(title: string, subtitle: string, onClick: () => void) {
    Row() {
      Column() {
        Text(title)
          .fontSize(16)
          .fontColor(this.themeTextPrimary)
        Text(subtitle)
          .fontSize(12)
          .fontColor(this.themeTextTertiary)
          .margin({ top: 2 })
      }
      .alignItems(HorizontalAlign.Start)
      .layoutWeight(1)

      Text('>')
        .fontSize(16)
        .fontColor('#CCCCCC')
    }
    .width('100%')
    .padding({ left: 16, right: 16, top: 14, bottom: 14 })
    .backgroundColor(this.themeBgCard)
    .onClick(onClick)
  }

  build() {
    Stack({ alignContent: Alignment.TopStart }) {
      Scroll() {
        Column({ space: 12 }) {
          Column({ space: 8 }) {
            Stack() {
              Image($r('app.media.colorfulO288'))
                .width(80)
                .height(80)
                .borderRadius(20)
            }
            Text('O-Code Controller')
              .fontSize(22)
              .fontWeight(FontWeight.Bold)
            Text('v0.1.0-alpha')
              .fontSize(13)
              .fontColor(this.themeTextTertiary)
          }
          .width('100%')
          .padding({ top: 40, bottom: 24 })
          .alignItems(HorizontalAlign.Center)

          Column({ space: 0 }) {
            this.ListItem('关于', '了解项目介绍与使用方法', () => { this.showAbout = true; })
            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })
            this.ListItem('账户', '登录或注册后端账户', () => { this.showAccount = true; this.loadAccountInfo(); })
            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })
            this.ListItem('远程环境', '管理远程后端容器', () => { router.pushUrl({ url: 'pages/ContainerList' }); })
            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })
            this.ListItem('软件更新', '检查并下载最新版本', () => { this.showUpdate = true; })
            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })
            this.ListItem('设置', this.isImmersiveMode ? '当前: 沉浸模式' : '当前: 完整模式', () => { this.showSettings = true; })
          }
          .width('100%')
          .borderRadius(12)
          .clip(true)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 16, bottom: 32 })
      }
      .width('100%')
      .height('100%')
      .backgroundColor(this.themeBgPrimary)

      if (this.showAbout) {
        this.buildAboutPanel()
      }

      if (this.showSettings) {
        this.buildSettingsPanel()
      }

      if (this.showAccount) {
        this.buildAccountPanel()
      }

      if (this.showUpdate) {
        this.buildUpdatePanel()
      }
    }
    .width('100%')
    .height('100%')
  }

  @Builder
  buildAboutPanel() {
    Column() {
      Column() {
        Row() {
          Text('<')
            .fontSize(22)
            .fontColor(this.themeAccent)
            .padding({ left: 4, right: 12 })
            .onClick(() => { this.showAbout = false; })
          Text('关于')
            .fontSize(18)
            .fontWeight(FontWeight.Bold)
            .layoutWeight(1)
        }
        .width('100%')
        .padding({ left: 20, right: 20, top: 16, bottom: 12 })

        Divider().height(0.5).color(this.themeBorderPrimary)
      }
      .width('100%')
      .backgroundColor(this.themeBgCard)

      Scroll() {
        Column({ space: 16 }) {
          Column({ space: 8 }) {
            Stack() {
              Image($r('app.media.colorfulO288'))
                .width(64)
                .height(64)
                .borderRadius(16)
            }
            Text('O-Code Controller')
              .fontSize(18)
              .fontWeight(FontWeight.Bold)
            Text('v0.1.0-alpha')
              .fontSize(13)
              .fontColor(this.themeTextTertiary)
          }
          .width('100%')
          .padding({ top: 24, bottom: 16 })
          .alignItems(HorizontalAlign.Center)

          this.InfoCard('关于', '一个基于 HarmonyOS 原生开发的 AI 编程助手客户端。\n\n通过连接你电脑上运行的 OpenCode 后端服务，你可以在手机上随时随地向 AI 发送编程指令，实时控制代码仓库。\n\n本项目基于 OpenCode Web 功能进行二次封装开发。\n\n本项目与 OpenCode 官方团队无任何关联，属于第三方独立项目。OpenCode 为 github.com/anomalyco/opencode 旗下的开源项目。')

          Column({ space: 8 }) {
            Text('快速上手')
              .fontSize(13)
              .fontColor(this.themeTextTertiary)
              .fontWeight(FontWeight.Medium)
              .width('100%')

            Text() {
              Span('① 在电脑上启动服务:\n')
              Span('opencode web --hostname 0.0.0.0 --port 4096\n\n')
              Span('② 手机与电脑连接（任选一种）:\n\n')
              Span('【同局域网】确保手机和电脑在同一 WiFi 下，在 App "后端" 中填入 http://电脑IP:4096\n\n')
              Span('【远程访问】推荐使用 ngrok 内网穿透:\n')
              Span('1. 电脑环境配置参考 ')
              Span('ngrok 官方指南')
                .fontColor(this.themeLink)
                .decoration({ type: TextDecorationType.Underline, color: this.themeLink })
                .onClick(() => this.openUrl('https://dashboard.ngrok.com/get-started/setup/linux'))
              Span('\n2. 运行: ')
              Span('ngrok http 4096')
              Span('\n3. 复制 ngrok 提供的访问地址并填入 App 中。\n\n')
              Span('⚠️ 强烈建议开启 Auth 认证以保障安全。\n\n')
              Span('【开机自启】可参考 ')
              Span('FastOpenCodeConfig')
                .fontColor(this.themeLink)
                .decoration({ type: TextDecorationType.Underline, color: this.themeLink })
                .onClick(() => this.openUrl('https://github.com/Liyulingyue/CreativeProjects/tree/main/FastOpenCodeConfig'))
              Span(' 中的脚本实现 Linux 服务化。')
            }
            .fontSize(14)
            .fontColor(this.themeTextPrimary)
            .lineHeight(22)
            .width('100%')
          }
          .width('100%')
          .padding(16)
          .backgroundColor(this.themeBgCard)
          .borderRadius(12)

          this.InfoCard('功能', '• 后端收藏夹管理\n• 一键连接 OpenCode 服务\n• 内嵌浏览器体验完整功能\n• 自动注入鉴权 Token')

          Column({ space: 8 }) {
            Text('开源参考')
              .fontSize(13)
              .fontColor(this.themeTextTertiary)
              .fontWeight(FontWeight.Medium)
              .width('100%')

            Text() {
              Span('OpenCode 后端: ')
              Span('anomalyco/opencode')
                .fontColor(this.themeLink)
                .decoration({ type: TextDecorationType.Underline, color: this.themeLink })
                .onClick(() => this.openUrl('https://github.com/anomalyco/opencode'))
              Span('\n\n本项目定位为 OpenCode 的移动端客户端，仅负责 UI 呈现和指令下发。')
            }
            .fontSize(14)
            .fontColor(this.themeTextPrimary)
            .lineHeight(22)
            .width('100%')
          }
          .width('100%')
          .padding(16)
          .backgroundColor(this.themeBgCard)
          .borderRadius(12)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 16, bottom: 32 })
      }
      .layoutWeight(1)
    }
    .width('100%')
    .height('100%')
    .backgroundColor(this.themeBgPrimary)
  }

  @Builder
  buildAccountPanel() {
    Stack() {
      Column() {
        Column() {
          Row() {
            Text('<')
              .fontSize(22)
              .fontColor(this.themeAccent)
              .padding({ left: 4, right: 12 })
              .onClick(() => { this.showAccount = false; })
            Text('账户')
              .fontSize(18)
              .fontWeight(FontWeight.Bold)
              .layoutWeight(1)
            Text('···')
              .fontSize(22)
              .fontColor(this.themeTextSecondary)
              .padding({ left: 12, right: 4 })
              .onClick(() => {
                this.configServerUrl = this.accountServerUrl;
                this.showServerConfig = true;
              })
          }
          .width('100%')
          .padding({ left: 20, right: 20, top: 16, bottom: 12 })

          Divider().height(0.5).color(this.themeBorderPrimary)
        }
        .width('100%')
        .backgroundColor(this.themeBgCard)

        Scroll() {
          Column({ space: 12 }) {
            Column({ space: 8 }) {
              Stack({ alignContent: Alignment.Center }) {
                Circle().width(80).height(80).fill(this.themeAccent + '22')
                Text('👤').fontSize(40)
              }
              Text('账户')
                .fontSize(22)
                .fontWeight(FontWeight.Bold)
              Text(this.isLoggedIn && this.currentUser ? `已登录: ${this.currentUser.username}` : '连接账户服务器')
                .fontSize(13)
                .fontColor(this.themeTextTertiary)
            }
            .width('100%')
            .padding({ top: 24, bottom: 16 })
            .alignItems(HorizontalAlign.Center)

            if (!this.accountServerUrl) {
              Column({ space: 8 }) {
                Text('⚠️ 未配置账户服务器')
                  .fontSize(16)
                  .fontColor(this.themeTextSecondary)
                Text('点击右上角 ··· 配置服务器地址')
                  .fontSize(13)
                  .fontColor(this.themeTextTertiary)
              }
              .width('100%')
              .padding(24)
              .alignItems(HorizontalAlign.Center)
              .backgroundColor(this.themeBgCard)
              .borderRadius(12)
            } else if (this.isLoggedIn && this.currentUser) {
              this.buildLoggedInView()
            } else {
              this.buildAuthForm()
            }
          }
          .width('100%')
          .padding({ left: 16, right: 16, top: 16, bottom: 32 })
        }
        .layoutWeight(1)
      }
      .width('100%')
      .height('100%')
      .backgroundColor(this.themeBgPrimary)

      if (this.showServerConfig) {
        Column() {
          Column() {
            Row() {
              Text('取消')
                .fontSize(16)
                .fontColor(this.themeTextSecondary)
                .onClick(() => { this.showServerConfig = false; })
              Text('服务器配置')
                .fontSize(18)
                .fontWeight(FontWeight.Bold)
                .layoutWeight(1)
                .textAlign(TextAlign.Center)
              Text('保存')
                .fontSize(16)
                .fontColor(this.themeAccent)
                .onClick(() => this.saveServerConfig())
            }
            .width('100%')
            .padding({ left: 20, right: 20, top: 16, bottom: 12 })

            Divider().height(0.5).color(this.themeBorderPrimary)
          }
          .width('100%')
          .backgroundColor(this.themeBgCard)

          Column({ space: 16 }) {
            Text('账户服务器地址')
              .fontSize(13)
              .fontColor(this.themeTextTertiary)
              .fontWeight(FontWeight.Medium)
              .width('100%')
              .padding({ left: 4 })

            TextInput({ placeholder: 'https://ocodecontroller.liyulingyue.com', text: this.configServerUrl })
              .width('100%')
              .height(48)
              .backgroundColor(this.themeBgCard)
              .borderRadius(12)
              .padding({ left: 16, right: 16 })
              .onChange((v: string) => { this.configServerUrl = v; })
          }
          .width('100%')
          .padding(20)
        }
    .width('100%')
    .height('100%')
    .backgroundColor(this.themeBgPrimary)
  }
    }
    .width('100%')
    .height('100%')
  }

  @Builder
  buildLoggedInView() {
    Column({ space: 12 }) {
      Column({ space: 8 }) {
        Row() {
          Text('当前用户')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
            .fontWeight(FontWeight.Medium)
          Blank()
          Text(this.currentUser?.plan ?? '')
            .fontSize(13)
            .fontColor(this.themeAccent)
            .fontWeight(FontWeight.Medium)
        }
        .width('100%')
        .height(24)
        .alignItems(VerticalAlign.Center)
        .padding({ left: 12, right: 12 })
        .margin({ top: 4 })

        Divider().height(1).color('#E0E0E0').margin({ left: 4, right: 4 })

        Row() {
          Stack({ alignContent: Alignment.Center }) {
            Circle().width(48).height(48).fill(this.themeAccent + '22')
            Text((this.currentUser?.username ?? 'U').charAt(0).toUpperCase())
              .fontSize(22)
              .fontWeight(FontWeight.Bold)
              .fontColor(this.themeAccent)
          }

          Column({ space: 4 }) {
            Text(this.currentUser?.username ?? '')
              .fontSize(18)
              .fontColor(this.themeTextPrimary)
              .fontWeight(FontWeight.Medium)
            Text(`ID: ${this.currentUser?.id ?? ''}`)
              .fontSize(12)
              .fontColor(this.themeTextTertiary)
          }
          .alignItems(HorizontalAlign.Start)
          .layoutWeight(1)
          .margin({ left: 12 })
        }
        .width('100%')
        .padding(16)
        .backgroundColor(this.themeBgCard)
        .borderRadius(12)
      }
      .width('100%')

      Column({ space: 0 }) {
        Row() {
          Column() {
            Text('升级用户组')
              .fontSize(13)
              .fontColor(this.themeTextTertiary)
              .fontWeight(FontWeight.Medium)
            TextInput({ placeholder: '输入兑换码', text: this.redeemCode })
              .height(40)
              .fontSize(14)
              .backgroundColor(this.themeBgPrimary)
              .borderRadius(8)
              .margin({ top: 6 })
              .onChange((value: string) => {
                this.redeemCode = value;
              })
          }
          .alignItems(HorizontalAlign.Start)
          .layoutWeight(1)
          .margin({ right: 8 })

          Column() {
            Text('')
              .height(13)
            Button('兑换')
              .height(40)
              .fontSize(13)
              .fontColor('#FFFFFF')
              .backgroundColor(this.themeAccent)
              .borderRadius(8)
              .enabled(!this.syncLoading)
              .onClick(() => this.doRedeem())
          }
          .alignItems(HorizontalAlign.End)
        }
        .width('100%')
        .padding(16)
        .backgroundColor(this.themeBgCard)
        .borderRadius(12)

        if (this.redeemMsg) {
          Text(this.redeemMsg)
            .fontSize(12)
            .fontColor(this.redeemMsg.includes('成功') ? this.themeAccent : '#FF3B30')
            .padding({ left: 16, bottom: 8 })
        }
      }
      .width('100%')

      // Column({ space: 0 }) {
      //   Row() {
      //     Column() {
      //       Text('账户服务器')
      //         .fontSize(16)
      //         .fontColor(this.themeTextPrimary)
      //       Text(this.accountServerUrl)
      //         .fontSize(12)
      //         .fontColor(this.themeTextTertiary)
      //         .margin({ top: 2 })
      //     }
      //     .alignItems(HorizontalAlign.Start)
      //     .layoutWeight(1)
      //   }
      //   .width('100%')
      //   .padding({ left: 16, right: 16, top: 14, bottom: 14 })
      // }
      // .width('100%')
      // .backgroundColor(this.themeBgCard)
      // .borderRadius(12)

      Column({ space: 8 }) {
        Button('保存当前配置到服务器')
          .width('100%')
          .height(44)
          .fontSize(14)
          .fontColor('#FFFFFF')
          .backgroundColor(this.themeAccent)
          .borderRadius(12)
          .enabled(!this.syncLoading)
          .onClick(() => this.doSaveConfig())

        Button('获取服务器配置到本机')
          .width('100%')
          .height(44)
          .fontSize(14)
          .fontColor('#FFFFFF')
          .backgroundColor(this.themeConfig)
          .borderRadius(12)
          .enabled(!this.syncLoading)
          .onClick(() => this.doSyncConfig())
      }
      .width('100%')

      Button('退出登录')
        .width('100%')
        .height(48)
        .fontSize(16)
        .fontColor('#FFFFFF')
        .backgroundColor('#FF3B30')
        .borderRadius(12)
        .onClick(() => this.logout())
    }
    .width('100%')
  }

  @Builder
  buildAuthForm() {
    Column({ space: 12 }) {
      if (this.activeForm === 'login') {
        this.buildLoginForm()
      } else {
        this.buildRegisterForm()
      }
    }
    .width('100%')
  }

  @Builder
  buildLoginForm() {
    Column({ space: 12 }) {
      Text('登录')
        .fontSize(13)
        .fontColor(this.themeTextTertiary)
        .fontWeight(FontWeight.Medium)
        .width('100%')
        .padding({ left: 4 })

      Column({ space: 0 }) {
        TextInput({ placeholder: '用户名', text: this.loginUsername })
          .width('100%')
          .height(48)
          .backgroundColor(this.themeBgCard)
          .borderRadius(0)
          .border({ width: { bottom: 0.5 }, color: this.themeBorderSecondary })
          .onChange((v: string) => { this.loginUsername = v; })

        TextInput({ placeholder: '密码', text: this.loginPassword })
          .width('100%')
          .height(48)
          .backgroundColor(this.themeBgCard)
          .borderRadius(0)
          .type(InputType.Password)
          .onChange((v: string) => { this.loginPassword = v; })
      }
      .width('100%')
      .backgroundColor(this.themeBgCard)
      .borderRadius(12)

      if (this.errorMsg) {
        Text(this.errorMsg)
          .fontSize(13)
          .fontColor('#FF3B30')
          .width('100%')
          .padding({ left: 4 })
      }

      Button(this.isLoading ? '登录中...' : '登录')
        .width('100%')
        .height(48)
        .fontSize(16)
        .fontColor('#FFFFFF')
        .backgroundColor(this.themeAccent)
        .borderRadius(12)
        .enabled(!this.isLoading)
        .onClick(() => this.doLogin())

      Text('没有账号？')
        .fontSize(13)
        .fontColor(this.themeTextTertiary)
        .width('100%')
        .textAlign(TextAlign.Center)
        .onClick(() => {
          this.activeForm = 'register';
          this.errorMsg = '';
        })
    }
    .width('100%')
  }

  @Builder
  buildRegisterForm() {
    Column({ space: 12 }) {
      Text('注册')
        .fontSize(13)
        .fontColor(this.themeTextTertiary)
        .fontWeight(FontWeight.Medium)
        .width('100%')
        .padding({ left: 4 })

      Column({ space: 0 }) {
        TextInput({ placeholder: '用户名', text: this.registerUsername })
          .width('100%')
          .height(48)
          .backgroundColor(this.themeBgCard)
          .borderRadius(0)
          .border({ width: { bottom: 0.5 }, color: this.themeBorderSecondary })
          .onChange((v: string) => { this.registerUsername = v; })

        TextInput({ placeholder: '密码（至少6位）', text: this.registerPassword })
          .width('100%')
          .height(48)
          .backgroundColor(this.themeBgCard)
          .borderRadius(0)
          .type(InputType.Password)
          .onChange((v: string) => { this.registerPassword = v; })

        TextInput({ placeholder: '确认密码', text: this.registerConfirm })
          .width('100%')
          .height(48)
          .backgroundColor(this.themeBgCard)
          .borderRadius(0)
          .type(InputType.Password)
          .onChange((v: string) => { this.registerConfirm = v; })
      }
      .width('100%')
      .backgroundColor(this.themeBgCard)
      .borderRadius(12)

      if (this.registerError) {
        Text(this.registerError)
          .fontSize(13)
          .fontColor('#FF3B30')
          .width('100%')
          .padding({ left: 4 })
      }

      Button(this.isLoading ? '注册中...' : '注册')
        .width('100%')
        .height(48)
        .fontSize(16)
        .fontColor('#FFFFFF')
        .backgroundColor(this.themeAccent)
        .borderRadius(12)
        .enabled(!this.isLoading)
        .onClick(() => this.doRegister())

      Text('已有账号？')
        .fontSize(13)
        .fontColor(this.themeTextTertiary)
        .width('100%')
        .textAlign(TextAlign.Center)
        .onClick(() => {
          this.activeForm = 'login';
          this.registerError = '';
        })
    }
    .width('100%')
  }

  @Builder
  buildSettingsPanel() {
    Column() {
      Column() {
        Row() {
          Text('<')
            .fontSize(22)
            .fontColor(this.themeAccent)
            .padding({ left: 4, right: 12 })
            .onClick(() => { this.showSettings = false; })
          Text('设置')
            .fontSize(18)
            .fontWeight(FontWeight.Bold)
            .layoutWeight(1)
        }
        .width('100%')
        .padding({ left: 20, right: 20, top: 16, bottom: 12 })

        Divider().height(0.5).color(this.themeBorderPrimary)
      }
      .width('100%')
      .backgroundColor(this.themeBgCard)

      Scroll() {
        Column({ space: 12 }) {
          Text('聊天展示模式')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          Column({ space: 0 }) {
            Row() {
              Column() {
                Text('沉浸模式')
                  .fontSize(16)
                  .fontColor(this.themeTextPrimary)
                Text('仅显示对话内容，最大化阅读空间')
                  .fontSize(12)
                  .fontColor(this.themeTextTertiary)
                  .margin({ top: 2 })
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)

              if (this.isImmersiveMode) {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.isImmersiveMode = true;
              this.core.setImmersiveMode(true);
            })

            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })

            Row() {
              Column() {
                Text('完整模式')
                  .fontSize(16)
                  .fontColor(this.themeTextPrimary)
                Text('显示用户名、时间等信息')
                  .fontSize(12)
                  .fontColor(this.themeTextTertiary)
                  .margin({ top: 2 })
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)

              if (!this.isImmersiveMode) {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.isImmersiveMode = false;
              this.core.setImmersiveMode(false);
            })
          }
          .width('100%')
          .backgroundColor(this.themeBgCard)
          .borderRadius(12)

          Text('输出框渲染')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          Column({ space: 0 }) {
            Row() {
              Text('文本')
                .fontSize(16)
                .fontColor(this.themeTextPrimary)
              Blank()
              if (!this.enableMarkdownRender) {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.enableMarkdownRender = false;
              AppStorage.setOrCreate<boolean>('enableMarkdownRender', false);
            })

            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })

            Row() {
              Text('Markdown')
                .fontSize(16)
                .fontColor(this.themeTextPrimary)
              Blank()
              if (this.enableMarkdownRender) {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.enableMarkdownRender = true;
              AppStorage.setOrCreate<boolean>('enableMarkdownRender', true);
            })
          }
          .width('100%')
          .backgroundColor(this.themeBgCard)
          .borderRadius(12)

          Text('历史消息加载')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          Column({ space: 0 }) {
            Row() {
              Column() {
                Text('全部消息')
                  .fontSize(16)
                  .fontColor(this.themeTextPrimary)
                Text('加载所有历史消息，渲染压力较大')
                  .fontSize(12)
                  .fontColor(this.themeTextTertiary)
                  .margin({ top: 2 })
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)

              if (this.messageLoadMode === 'all') {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.messageLoadMode = 'all';
              this.core.setMessageLoadMode('all');
            })

            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })

            Row() {
              Column() {
                Text('仅最新20条')
                  .fontSize(16)
                  .fontColor(this.themeTextPrimary)
                Text('只加载最近20条消息，启动更快')
                  .fontSize(12)
                  .fontColor(this.themeTextTertiary)
                  .margin({ top: 2 })
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)

              if (this.messageLoadMode === 'latest') {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.messageLoadMode = 'latest';
              this.core.setMessageLoadMode('latest');
            })
          }
          .width('100%')
          .backgroundColor(this.themeBgCard)
          .borderRadius(12)

          Text('用户名显示')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          TextInput({ placeholder: '默认显示 User', text: this.displayUsername })
            .width('100%')
            .height(44)
            .backgroundColor(this.themeBgCard)
            .borderRadius(12)
            .padding({ left: 16, right: 16 })
            .onChange((v: string) => {
              this.displayUsername = v;
              this.core.setDisplayUsername(v);
            })

          Text('屏幕旋转')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          Column({ space: 0 }) {
            Row() {
              Column() {
                Text('跟随系统')
                  .fontSize(16)
                  .fontColor(this.themeTextPrimary)
                Text('根据重力感应自动旋转屏幕')
                  .fontSize(12)
                  .fontColor(this.themeTextTertiary)
                  .margin({ top: 2 })
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)

              if (!this.isRotationLocked) {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.isRotationLocked = false;
              this.core.setRotationLocked(false);
              AppStorage.setOrCreate<boolean>('rotationLocked', false);
            })

            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })

            Row() {
              Column() {
                Text('锁定竖屏')
                  .fontSize(16)
                  .fontColor(this.themeTextPrimary)
                Text('屏幕不会随手机旋转而改变方向')
                  .fontSize(12)
                  .fontColor(this.themeTextTertiary)
                  .margin({ top: 2 })
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)

              if (this.isRotationLocked) {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.isRotationLocked = true;
              this.core.setRotationLocked(true);
              AppStorage.setOrCreate<boolean>('rotationLocked', true);
            })
          }
          .width('100%')
          .backgroundColor(this.themeBgCard)
          .borderRadius(12)

          Text('颜色深浅')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          Column({ space: 0 }) {
            Row() {
              Text('浅色模式')
                .fontSize(16)
                .fontColor(this.themeTextPrimary)
              Blank()
              if (!this.isDarkMode) {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.isDarkMode = false;
              this.core.setDarkMode(false);
              this.core.applyTheme(this.currentThemeName, false);
            })

            Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })

            Row() {
              Text('深色模式')
                .fontSize(16)
                .fontColor(this.themeTextPrimary)
              Blank()
              if (this.isDarkMode) {
                Text('✓')
                  .fontSize(18)
                  .fontColor(this.themeAccent)
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.isDarkMode = true;
              this.core.setDarkMode(true);
              this.core.applyTheme(this.currentThemeName, true);
            })
          }
          .width('100%')
          .backgroundColor(this.themeBgCard)
          .borderRadius(12)

          Text('颜色主题')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          Row({ space: 16 }) {
            ForEach(OpenCodeCore.THEME_NAMES, (name: string) => {
              Column() {
                Stack() {
                  Circle().width(28).height(28).fill(this.getThemeColor(name))
                  if (this.currentThemeName === name) {
                    Text('✓').fontSize(12).fontColor('#FFFFFF')
                  }
                }
                .onClick(() => {
                  this.currentThemeName = name;
                  this.core.setThemeName(name);
                  this.core.applyTheme(name, this.isDarkMode);
                })
                Text(name)
                  .fontSize(10)
                  .fontColor(this.currentThemeName === name ? this.themeAccent : this.themeTextTertiary)
                  .margin({ top: 4 })
              }
              .width(42)
            })
          }
          .width('100%')
          .justifyContent(FlexAlign.Center)
          .padding(16)
          .backgroundColor(this.themeBgCard)
          .borderRadius(12)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 16, bottom: 32 })
      }
      .layoutWeight(1)
    }
    .width('100%')
    .height('100%')
    .backgroundColor(this.themeBgPrimary)
  }

  private doCheckUpdate(): void {
    this.updateCheckLoading = true;
    this.updateStatus = '检查中...';
    this.updateAvailable = false;
    this.updateSuccessUrls = [];
    this.updateCheckCompletedCount = 0;
    const totalCount = 2;
    const servers = ['https://www.liyulingyue.com', 'http://cn.liyulingyue.com'];

    const onSingleComplete = (): void => {
      this.updateCheckCompletedCount++;
      if (this.updateCheckCompletedCount >= totalCount) {
        this.updateCheckLoading = false;
        if (this.updateSuccessUrls.length === 0) {
          this.updateStatus = '检查失败，请稍后重试';
        } else if (!this.updateAvailable) {
          this.updateStatus = '已是最新版本';
        } else {
          const names: string[] = this.updateSuccessUrls.map((url: string): string => {
            return url.includes('www') ? '国际' : '国内';
          });
          this.updateStatus = `发现新版本: ${this.remoteVersion}（${names.join('、')}站可用）`;
        }
      }
    };

    const onSingleSuccess = (baseUrl: string, resp: UpdateTimestampResp): void => {
      this.updateSuccessUrls.push(baseUrl);
      this.remoteVersion = resp.timestamp;
      if (this.remoteVersion !== this.localVersion) {
        this.updateAvailable = true;
      }
      onSingleComplete();
    };

    this.checkServerCallback(servers[0], onSingleSuccess, onSingleComplete);
    this.checkServerCallback(servers[1], onSingleSuccess, onSingleComplete);
  }

  private checkServerCallback(baseUrl: string, onSuccess: (baseUrl: string, resp: UpdateTimestampResp) => void, onComplete: () => void): void {
    const req = http.createHttp();
    const timestampUrl = `${baseUrl}/api/pages/downloads/tags/OCodeController/timestamp`;
    req.request(
      timestampUrl,
      {
        method: http.RequestMethod.GET,
        connectTimeout: 10000,
        readTimeout: 10000
      },
      (err, data) => {
        req.destroy();
        if (err) {
          onComplete();
          return;
        }
        if (data.responseCode === 200) {
          try {
            const resp = JSON.parse(data.result as string) as UpdateTimestampResp;
            onSuccess(baseUrl, resp);
          } catch {
            onComplete();
          }
        } else {
          onComplete();
        }
      }
    );
  }

  private getDownloadUrls(): string[] {
    if (!this.updateAvailable) return [];
    return this.updateSuccessUrls.map((url: string): string => {
      return `${url}/api/pages/downloads/tags/OCodeController`;
    });
  }

  private getThemeColor(name: string): string {
    if (name === '极光绿') { return '#07C160'; }
    if (name === '樱花粉') { return '#FF69B4'; }
    if (name === '天空蓝') { return '#1E90FF'; }
    if (name === '日落橙') { return '#FF8C00'; }
    if (name === '薰衣草') { return '#9B59B6'; }
    return '#07C160';
  }

  @Builder
  buildUpdatePanel() {
    Stack() {
      Column() {
        Column() {
          Row() {
            Text('<')
              .fontSize(22)
              .fontColor(this.themeAccent)
              .padding({ left: 4, right: 12 })
              .onClick(() => { this.showUpdate = false; })
            Text('软件更新')
              .fontSize(18)
              .fontWeight(FontWeight.Bold)
              .layoutWeight(1)
          }
          .width('100%')
          .padding({ left: 20, right: 20, top: 16, bottom: 12 })

          Divider().height(0.5).color(this.themeBorderPrimary)
        }
        .width('100%')
        .backgroundColor(this.themeBgCard)

        Scroll() {
          Column({ space: 16 }) {
            Column({ space: 8 }) {
              Stack() {
                Image($r('app.media.colorfulO288'))
                  .width(64)
                  .height(64)
                  .borderRadius(16)
              }
              Text('O-Code Controller')
                .fontSize(18)
                .fontWeight(FontWeight.Bold)
              Text(`当前版本: ${this.localVersion}`)
                .fontSize(13)
                .fontColor(this.themeTextTertiary)
            }
            .width('100%')
            .padding({ top: 24, bottom: 16 })
            .alignItems(HorizontalAlign.Center)

            Column({ space: 12 }) {
              Button(this.updateCheckLoading ? '检查中...' : '检查更新')
                .width('100%')
                .height(44)
                .fontSize(14)
                .fontColor('#FFFFFF')
                .backgroundColor(this.themeAccent)
                .borderRadius(12)
                .enabled(!this.updateCheckLoading)
                .onClick(() => this.doCheckUpdate())
            }
            .width('100%')

            if (this.updateStatus) {
              Column({ space: 8 }) {
                Text(this.updateStatus)
                  .fontSize(14)
                  .fontColor(this.updateAvailable ? this.themeAccent : this.themeTextSecondary)
                  .width('100%')

                if (this.updateAvailable) {
                  ForEach(this.getDownloadUrls(), (url: string) => {
                    Column({ space: 4 }) {
                      Text(url.includes('www') ? '国际站下载' : '国内站下载')
                        .fontSize(12)
                        .fontColor(this.themeTextTertiary)
                        .width('100%')
                      Text(url)
                        .fontSize(13)
                        .fontColor(this.themeLink)
                        .width('100%')
                        .decoration({ type: TextDecorationType.Underline })
                        .maxLines(2)
                        .textOverflow({ overflow: TextOverflow.Ellipsis })
                        .onClick(() => {
                          try {
                            const context = this.core.getContext();
                            if (context) {
                              context.startAbility({
                                action: 'ohos.want.action.viewData',
                                uri: url
                              });
                            }
                          } catch (e) {
                            promptAction.showToast({ message: '请手动复制链接到浏览器', duration: 3000 });
                          }
                        })
                    }
                    .width('100%')
                    .padding(12)
                    .backgroundColor(this.themeBgPrimary)
                    .borderRadius(8)
                  }, (url: string) => url)
                }
              }
              .width('100%')
              .padding(16)
              .backgroundColor(this.themeBgCard)
              .borderRadius(12)
            }

            Text('提示：www站点中国区访问可能会较慢，cn站点对中国区访问更友好。两个站点内容一致，请按需选择下载链接。')
              .fontSize(12)
              .fontColor(this.themeTextTertiary)
              .width('100%')
          }
          .width('100%')
          .padding({ left: 16, right: 16, top: 16, bottom: 32 })
        }
        .layoutWeight(1)
      }
      .width('100%')
      .height('100%')
      .backgroundColor(this.themeBgPrimary)
    }
    .width('100%')
    .height('100%')
  }

  @Builder
  InfoCard(title: string, content: string) {
    Column({ space: 8 }) {
      Text(title)
        .fontSize(13)
        .fontColor(this.themeTextTertiary)
        .fontWeight(FontWeight.Medium)
        .width('100%')

      Text(content)
        .fontSize(14)
        .fontColor(this.themeTextPrimary)
        .lineHeight(22)
        .width('100%')
    }
    .width('100%')
    .padding(16)
    .backgroundColor(this.themeBgCard)
    .borderRadius(12)
  }
}

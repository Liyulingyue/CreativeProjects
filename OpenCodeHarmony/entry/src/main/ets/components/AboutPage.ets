import router from '@ohos.router';
import { OpenCodeCore } from '../core/OpenCodeCore';

@Component
export struct AboutPage {
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  @State isImmersiveMode: boolean = true;
  @State showAbout: boolean = false;
  @State showSettings: boolean = false;
  @State displayUsername: string = '';

  aboutToAppear() {
    this.isImmersiveMode = this.core.getImmersiveMode();
    this.displayUsername = this.core.getDisplayUsername();
  }

  private openUrl(url: string): void {
    console.info('[AboutPage] openUrl:', url);
  }

  private toggleMode() {
    this.isImmersiveMode = !this.isImmersiveMode;
    AppStorage.setOrCreate<boolean>('immersiveMode', this.isImmersiveMode);
  }

  @Builder
  ListItem(title: string, subtitle: string, onClick: () => void) {
    Row() {
      Column() {
        Text(title)
          .fontSize(16)
          .fontColor('#333333')
        Text(subtitle)
          .fontSize(12)
          .fontColor('#999999')
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
    .backgroundColor('#FFFFFF')
    .onClick(onClick)
  }

  build() {
    Stack({ alignContent: Alignment.TopStart }) {
      Scroll() {
        Column({ space: 12 }) {
          Column({ space: 8 }) {
            Stack() {
              Rect().fill('#07C160').width(80).height(80).borderRadius(20)
              Text('OC')
                .fontSize(28).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
            }
            Text('OpenCode Harmony')
              .fontSize(22)
              .fontWeight(FontWeight.Bold)
            Text('v0.1.0-alpha')
              .fontSize(13)
              .fontColor('#999999')
          }
          .width('100%')
          .padding({ top: 40, bottom: 24 })
          .alignItems(HorizontalAlign.Center)

          Column({ space: 0 }) {
            this.ListItem('关于', '了解项目介绍与使用方法', () => { this.showAbout = true; })
            Divider().height(0.5).color('#F0F0F0').margin({ left: 16 })
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
      .backgroundColor('#F5F5F5')

      if (this.showAbout) {
        this.buildAboutPanel()
      }

      if (this.showSettings) {
        this.buildSettingsPanel()
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
            .fontColor('#07C160')
            .padding({ left: 4, right: 12 })
            .onClick(() => { this.showAbout = false; })
          Text('关于')
            .fontSize(18)
            .fontWeight(FontWeight.Bold)
            .layoutWeight(1)
        }
        .width('100%')
        .padding({ left: 20, right: 20, top: 16, bottom: 12 })

        Divider().height(0.5).color('#E0E0E0')
      }
      .width('100%')
      .backgroundColor('#FFFFFF')

      Scroll() {
        Column({ space: 16 }) {
          Column({ space: 8 }) {
            Stack() {
              Rect().fill('#07C160').width(64).height(64).borderRadius(16)
              Text('OC')
                .fontSize(22).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
            }
            Text('OpenCode Harmony')
              .fontSize(18)
              .fontWeight(FontWeight.Bold)
            Text('v0.1.0-alpha')
              .fontSize(13)
              .fontColor('#999999')
          }
          .width('100%')
          .padding({ top: 24, bottom: 16 })
          .alignItems(HorizontalAlign.Center)

          this.InfoCard('关于', '一个基于 HarmonyOS 原生开发的 AI 编程助手客户端。\n\n通过连接你电脑上运行的 OpenCode 后端服务，你可以在手机上随时随地向 AI 发送编程指令，实时控制代码仓库。\n\n本项目基于 OpenCode Web 功能进行二次封装开发。\n\n本项目与 OpenCode 官方团队无任何关联，属于第三方独立项目。OpenCode 为 github.com/anomalyco/opencode 旗下的开源项目。')

          Column({ space: 8 }) {
            Text('快速上手')
              .fontSize(13)
              .fontColor('#999999')
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
                .fontColor('#007DFF')
                .decoration({ type: TextDecorationType.Underline, color: '#007DFF' })
                .onClick(() => this.openUrl('https://dashboard.ngrok.com/get-started/setup/linux'))
              Span('\n2. 运行: ')
              Span('ngrok http 4096')
              Span('\n3. 复制 ngrok 提供的访问地址并填入 App 中。\n\n')
              Span('⚠️ 强烈建议开启 Auth 认证以保障安全。\n\n')
              Span('【开机自启】可参考 ')
              Span('FastOpenCodeConfig')
                .fontColor('#007DFF')
                .decoration({ type: TextDecorationType.Underline, color: '#007DFF' })
                .onClick(() => this.openUrl('https://github.com/Liyulingyue/CreativeProjects/tree/main/FastOpenCodeConfig'))
              Span(' 中的脚本实现 Linux 服务化。')
            }
            .fontSize(14)
            .fontColor('#333333')
            .lineHeight(22)
            .width('100%')
          }
          .width('100%')
          .padding(16)
          .backgroundColor('#FFFFFF')
          .borderRadius(12)

          this.InfoCard('功能', '• 后端收藏夹管理\n• 一键连接 OpenCode 服务\n• 内嵌浏览器体验完整功能\n• 自动注入鉴权 Token')

          Column({ space: 8 }) {
            Text('开源参考')
              .fontSize(13)
              .fontColor('#999999')
              .fontWeight(FontWeight.Medium)
              .width('100%')

            Text() {
              Span('OpenCode 后端: ')
              Span('anomalyco/opencode')
                .fontColor('#007DFF')
                .decoration({ type: TextDecorationType.Underline, color: '#007DFF' })
                .onClick(() => this.openUrl('https://github.com/anomalyco/opencode'))
              Span('\n\n本项目定位为 OpenCode 的移动端客户端，仅负责 UI 呈现和指令下发。')
            }
            .fontSize(14)
            .fontColor('#333333')
            .lineHeight(22)
            .width('100%')
          }
          .width('100%')
          .padding(16)
          .backgroundColor('#FFFFFF')
          .borderRadius(12)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 16, bottom: 32 })
      }
      .layoutWeight(1)
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#F5F5F5')
  }

  @Builder
  buildSettingsPanel() {
    Column() {
      Column() {
        Row() {
          Text('<')
            .fontSize(22)
            .fontColor('#07C160')
            .padding({ left: 4, right: 12 })
            .onClick(() => { this.showSettings = false; })
          Text('设置')
            .fontSize(18)
            .fontWeight(FontWeight.Bold)
            .layoutWeight(1)
        }
        .width('100%')
        .padding({ left: 20, right: 20, top: 16, bottom: 12 })

        Divider().height(0.5).color('#E0E0E0')
      }
      .width('100%')
      .backgroundColor('#FFFFFF')

      Scroll() {
        Column({ space: 12 }) {
          Text('聊天展示模式')
            .fontSize(13)
            .fontColor('#999999')
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          Column({ space: 0 }) {
            Row() {
              Column() {
                Text('沉浸模式')
                  .fontSize(16)
                  .fontColor('#333333')
                Text('仅显示对话内容，最大化阅读空间')
                  .fontSize(12)
                  .fontColor('#999999')
                  .margin({ top: 2 })
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)

              if (this.isImmersiveMode) {
                Text('✓')
                  .fontSize(18)
                  .fontColor('#07C160')
              }
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 14, bottom: 14 })
            .onClick(() => {
              this.isImmersiveMode = true;
              this.core.setImmersiveMode(true);
            })

            Divider().height(0.5).color('#F0F0F0').margin({ left: 16 })

            Row() {
              Column() {
                Text('完整模式')
                  .fontSize(16)
                  .fontColor('#333333')
                Text('显示用户名、时间等信息')
                  .fontSize(12)
                  .fontColor('#999999')
                  .margin({ top: 2 })
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)

              if (!this.isImmersiveMode) {
                Text('✓')
                  .fontSize(18)
                  .fontColor('#07C160')
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
          .backgroundColor('#FFFFFF')
          .borderRadius(12)

          Text('用户名显示')
            .fontSize(13)
            .fontColor('#999999')
            .fontWeight(FontWeight.Medium)
            .width('100%')
            .padding({ left: 4, top: 16, bottom: 8 })

          TextInput({ placeholder: '默认显示 User', text: this.displayUsername })
            .width('100%')
            .height(44)
            .backgroundColor('#FFFFFF')
            .borderRadius(12)
            .padding({ left: 16, right: 16 })
            .onChange((v: string) => {
              this.displayUsername = v;
              this.core.setDisplayUsername(v);
            })
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 16, bottom: 32 })
      }
      .layoutWeight(1)
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#F5F5F5')
  }

  @Builder
  InfoCard(title: string, content: string) {
    Column({ space: 8 }) {
      Text(title)
        .fontSize(13)
        .fontColor('#999999')
        .fontWeight(FontWeight.Medium)
        .width('100%')

      Text(content)
        .fontSize(14)
        .fontColor('#333333')
        .lineHeight(22)
        .width('100%')
    }
    .width('100%')
    .padding(16)
    .backgroundColor('#FFFFFF')
    .borderRadius(12)
  }
}

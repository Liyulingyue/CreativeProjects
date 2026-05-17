import { common, Want } from '@kit.AbilityKit';

@Component
export struct AboutPage {
  private context = getContext(this) as common.UIAbilityContext;

  private openUrl(url: string) {
    let want: Want = {
      action: 'ohos.want.action.viewData',
      entities: ['entity.system.browsable'],
      uri: url
    };
    this.context.startAbility(want);
  }

  build() {
    Scroll() {
      Column({ space: 16 }) {
        // App 图标区域
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

        // 简介卡片
        this.InfoCard('关于', '一个基于 HarmonyOS 原生开发的 AI 编程助手客户端。\n\n通过连接你电脑上运行的 OpenCode 后端服务，你可以在手机上随时随地向 AI 发送编程指令，实时控制代码仓库。\n\n本项目基于 OpenCode Web 功能进行二次封装开发。\n\n本项目与 OpenCode 官方团队无任何关联，属于第三方独立项目。OpenCode 为 github.com/anomalyco/opencode 旗下的开源项目。')

        // 快速上手卡片
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


        // 功能列表
        this.InfoCard('功能', '• 后端收藏夹管理\n• 一键连接 OpenCode 服务\n• 内嵌浏览器体验完整功能\n• 自动注入鉴权 Token')

        // 开源参考卡片
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

        // Text('MIT License · 2026')
        //   .fontSize(12)
        //   .fontColor('#CCCCCC')
        //   .margin({ top: 8, bottom: 32 })
      }
      .width('100%')
      .padding({ left: 16, right: 16 })
    }
    .width('100%')
    .backgroundColor('#F5F5F5')
  }

  @Builder InfoCard(title: string, content: string) {
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

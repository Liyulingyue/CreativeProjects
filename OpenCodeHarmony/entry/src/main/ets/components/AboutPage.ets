@Component
export struct AboutPage {
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
        this.InfoCard('关于', '一个基于 HarmonyOS 原生开发的 AI 编程助手客户端。\n\n通过连接你电脑上运行的 OpenCode 后端服务，你可以在手机上随时随地向 AI 发送编程指令，实时控制代码仓库。')

        // 功能列表
        this.InfoCard('功能', '• 后端收藏夹管理\n• 一键连接 OpenCode 服务\n• 内嵌浏览器体验完整功能\n• 自动注入鉴权 Token')

        // 技术信息
        this.InfoCard('技术栈', '• HarmonyOS SDK 5.0+\n• ArkTS / ArkUI\n• OpenCode 后端 API\n• Monorepo 解耦架构')

        // 开源说明
        this.InfoCard('开源声明', '本项目基于 OpenCode Web 功能进行二次封装开发。\n\n本项目与 OpenCode 官方团队无任何关联，属于第三方独立项目。OpenCode 为 github.com/anomalyco/opencode 旗下的开源项目。')

        // 链接
        this.InfoCard('开源参考', 'OpenCode 后端: github.com/anomalyco/opencode\n\n本项目定位为 OpenCode 的移动端原生客户端，仅负责 UI 呈现和指令下发。')

        Text('MIT License · 2026')
          .fontSize(12)
          .fontColor('#CCCCCC')
          .margin({ top: 8, bottom: 32 })
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

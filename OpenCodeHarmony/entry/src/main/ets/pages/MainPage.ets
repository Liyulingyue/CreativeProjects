import { SessionList } from '../components/SessionList';
import { BackendList } from '../components/BackendList';
import { AboutPage } from '../components/AboutPage';

@Entry
@Component
struct MainPage {
  @State currentTab: number = 0;
  @StorageLink('needRefreshSessions') needRefreshSessions: string = 'false';

  onPageShow() {
    // 页面显示时检查是否需要刷新会话列表
    if (this.needRefreshSessions === 'true') {
      this.needRefreshSessions = 'false';
      AppStorage.SetOrCreate<string>('refreshSessionsNow', Date.now().toString());
    }
  }

  @Builder TabBar(index: number, icon: string, label: string) {
    Column({ space: 4 }) {
      Text(icon)
        .fontSize(22)
        .fontColor(this.currentTab === index ? '#07C160' : '#888888')
      Text(label)
        .fontSize(10)
        .fontColor(this.currentTab === index ? '#07C160' : '#888888')
    }
    .padding({ top: 8, bottom: 8 })
    .layoutWeight(1)
    .alignItems(HorizontalAlign.Center)
    .onClick(() => this.currentTab = index)
  }


  build() {
    Column() {
      if (this.currentTab === 0) {
        SessionList()
          .layoutWeight(1)
      } else if (this.currentTab === 1) {
        BackendList()
          .layoutWeight(1)
      } else {
        AboutPage()
          .layoutWeight(1)
      }

      Row() {
        this.TabBar(0, '💬', '会话')
        this.TabBar(1, '🖥', '后端')
        this.TabBar(2, 'ℹ', '关于')
      }
      .width('100%')
      .height(60)
      .backgroundColor('#FFFFFF')
      .border({ width: { top: 0.5 }, color: '#E0E0E0' })
    }
    .width('100%')
    .height('100%')
  }
}

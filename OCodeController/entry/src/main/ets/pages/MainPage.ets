import { SessionList } from '../components/SessionList';
import { BackendList } from '../components/BackendList';
import { AboutPage } from '../components/AboutPage';

@Entry
@Component
struct MainPage {
  @State currentTab: number = 0;
  @StorageLink('needRefreshSessions') needRefreshSessions: string = '';
  @StorageLink('themeAccent') themeAccent: string = '#07C160';
  @StorageLink('themeBgCard') themeBgCard: string = '#FFFFFF';
  @StorageLink('themeBorderPrimary') themeBorderPrimary: string = '#E0E0E0';

  onBackPress(): boolean {
    const aboutOpen = AppStorage.get<boolean>('aboutPanelVisible') ?? false;
    const settingsOpen = AppStorage.get<boolean>('settingsPanelVisible') ?? false;
    if (aboutOpen) {
      AppStorage.setOrCreate<boolean>('aboutPanelVisible', false);
      return true;
    }
    if (settingsOpen) {
      AppStorage.setOrCreate<boolean>('settingsPanelVisible', false);
      return true;
    }
    return false;
  }

  onPageShow() {
    if (this.needRefreshSessions === 'true') {
      this.needRefreshSessions = '';
      AppStorage.setOrCreate<string>('refreshSessionsNow', Date.now().toString());
    }
  }

  @Builder TabBar(index: number, icon: string, label: string) {
    Column({ space: 4 }) {
      Text(icon)
        .fontSize(22)
        .fontColor(this.currentTab === index ? this.themeAccent : '#888888')
      Text(label)
        .fontSize(10)
        .fontColor(this.currentTab === index ? this.themeAccent : '#888888')
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
        this.TabBar(2, '⚙', '控制台')
      }
      .width('100%')
      .height(60)
      .backgroundColor(this.themeBgCard)
      .border({ width: { top: 0.5 }, color: this.themeBorderPrimary })
    }
    .width('100%')
    .height('100%')
  }
}

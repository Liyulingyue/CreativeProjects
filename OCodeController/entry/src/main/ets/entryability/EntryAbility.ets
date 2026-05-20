import { UIAbility, Want } from '@kit.AbilityKit';
import { window } from '@kit.ArkUI';
import { OpenCodeCore } from '../core/OpenCodeCore';

export default class EntryAbility extends UIAbility {
  async onCreate(want: Want, launchParam: Object): Promise<void> {
    console.info('[EntryAbility] Ability onCreate');
  }

  onDestroy(): void {
    console.info('[EntryAbility] Ability onDestroy');
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    console.info('[EntryAbility] Ability onWindowStageCreate');

    const core = OpenCodeCore.getInstance();
    core.setSessionsChangedCallback((timestamp: number) => {
      AppStorage.setOrCreate<string>('refreshSessionsNow', timestamp.toString());
    });

    windowStage.loadContent('pages/MainPage', (err) => {
      if (err.code) {
        console.error('[EntryAbility] Failed to load the content. Cause:', JSON.stringify(err));
        return;
      }
      console.info('[EntryAbility] Succeeded in loading the content.');
    });

    this.initCore(core);
  }

  private async initCore(core: OpenCodeCore): Promise<void> {
    try {
      await core.init(this.context);
    } catch (e) {
      console.error('[EntryAbility] init failed:', e);
    }
    core.applyTheme(core.getThemeName());
    AppStorage.setOrCreate<string>('refreshSessionsNow', Date.now().toString());
  }

  onWindowStageDestroy(): void {
    console.info('[EntryAbility] Ability onWindowStageDestroy');
  }

  onForeground(): void {
    console.info('[EntryAbility] Ability onForeground');
  }

  onBackground(): void {
    console.info('[EntryAbility] Ability onBackground');
  }
}

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

  async onWindowStageCreate(windowStage: window.WindowStage): Promise<void> {
    console.info('[EntryAbility] Ability onWindowStageCreate');

    const core = OpenCodeCore.getInstance();
    try {
      await core.init(this.context);
    } catch (e) {
      console.error('[EntryAbility] init failed:', e);
    }

    core.setSessionsChangedCallback((timestamp: number) => {
      AppStorage.setOrCreate<string>('needRefreshSessions', timestamp.toString());
    });

    windowStage.loadContent('pages/MainPage', (err) => {
      if (err.code) {
        console.error('[EntryAbility] Failed to load the content. Cause:', JSON.stringify(err));
        return;
      }
      console.info('[EntryAbility] Succeeded in loading the content.');
    });
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

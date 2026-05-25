import { OpenCodeCore, ContainerInfo } from '../core/OpenCodeCore';
import router from '@ohos.router';
import promptAction from '@ohos.promptAction';

@Entry
@Component
struct ContainerList {
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  @State containers: ContainerInfo[] = [];
  @State loading: boolean = false;
  @StorageLink('themeAccent') themeAccent: string = '#07C160';
  @StorageLink('themeBgPrimary') themeBgPrimary: string = '#F5F5F5';
  @StorageLink('themeBgSecondary') themeBgSecondary: string = '#EDEDED';
  @StorageLink('themeBgCard') themeBgCard: string = '#FFFFFF';
  @StorageLink('themeTextPrimary') themeTextPrimary: string = '#333333';
  @StorageLink('themeTextSecondary') themeTextSecondary: string = '#888888';
  @StorageLink('themeTextTertiary') themeTextTertiary: string = '#999999';
  @StorageLink('themeBorderPrimary') themeBorderPrimary: string = '#E0E0E0';
  @StorageLink('themeBorderSecondary') themeBorderSecondary: string = '#F0F0F0';
  @StorageLink('themeConfig') themeConfig: string = '#1677FF';
  dialogController: CustomDialogController | null = null;

  aboutToAppear() {
    this.loadContainers();
    this.dialogController = new CustomDialogController({
      builder: CreateContainerDialog({ onConfirm: (name: string) => this.doConfirmCreateFromDialog(name) }),
      alignment: DialogAlignment.Center,
      customStyle: true
    });
  }

  aboutToDisappear() {
    if (this.dialogController) {
      this.dialogController.close();
      this.dialogController = null;
    }
  }

  private async doCreate(): Promise<void> {
    if (this.dialogController) {
      this.dialogController.open();
    }
  }

  private async doConfirmCreateFromDialog(name: string): Promise<void> {
    const trimmed = name.trim();
    if (!trimmed) {
      promptAction.showToast({ message: '请输入名称', duration: 2000 });
      return;
    }
    this.loading = true;
    try {
      const result = await this.core.createContainer(trimmed);
      if (result.success && result.container) {
        this.containers = [...this.containers, result.container as ContainerInfo];
        const c = result.container as ContainerInfo;
        if (c.opencode_url && c.opencode_username) {
          await this.core.addBackendFromContainer(c.opencode_url, c.opencode_username, c.opencode_password || '', c.name);
        }
        AppStorage.setOrCreate<string>('needRefreshBackends', 'true');
        promptAction.showToast({ message: '创建成功', duration: 2000 });
      } else {
        promptAction.showToast({ message: result.error || '创建失败', duration: 3000 });
      }
    } catch (e) {
      promptAction.showToast({ message: '创建失败', duration: 3000 });
    } finally {
      this.loading = false;
    }
  }

  private async loadContainers(): Promise<void> {
    this.loading = true;
    const result = await this.core.listContainers();
    this.loading = false;
      if (result.success) {
      this.containers = result.containers;
    } else {
      promptAction.showToast({ message: result.error || '加载失败', duration: 3000 });
    }
  }

  private async doStart(id: string): Promise<void> {
    const result = await this.core.startContainer(id);
    if (result.success) {
      await this.loadContainers();
    } else {
      promptAction.showToast({ message: result.error || '启动失败', duration: 3000 });
    }
  }

  private async doStop(id: string): Promise<void> {
    const result = await this.core.stopContainer(id);
    if (result.success) {
      await this.loadContainers();
    } else {
      promptAction.showToast({ message: result.error || '停止失败', duration: 3000 });
    }
  }

  private async doDelete(id: string): Promise<void> {
    const result = await this.core.deleteContainer(id);
    if (result.success) {
      await this.loadContainers();
    } else {
      promptAction.showToast({ message: result.error || '删除失败', duration: 3000 });
    }
  }

  private async doAddAsBackend(c: ContainerInfo): Promise<void> {
    if (c.opencode_url && c.opencode_username) {
      await this.core.addBackendFromContainer(c.opencode_url, c.opencode_username, c.opencode_password || '', c.name);
      promptAction.showToast({ message: '已添加到后端列表', duration: 3000 });
    }
  }

  build() {
    Column() {
      Row() {
        Text('<')
          .fontSize(24)
          .fontColor(this.themeAccent)
          .padding({ right: 12 })
          .onClick(() => router.back())
        Text('远程环境')
          .fontSize(20)
          .fontWeight(FontWeight.Bold)
        Blank()
        Text('+')
          .fontSize(28)
          .fontColor(this.themeAccent)
          .padding({ left: 16, right: 16 })
          .onClick(() => this.doCreate())
      }
      .width('100%')
      .padding({ left: 12, right: 4, top: 15, bottom: 15 })
      .backgroundColor(this.themeBgSecondary)

      if (this.loading) {
        Column() {
          LoadingProgress()
            .width(48)
            .height(48)
          Text('加载中...')
            .fontSize(14)
            .fontColor(this.themeTextTertiary)
            .margin({ top: 12 })
        }
        .width('100%')
        .height('100%')
        .justifyContent(FlexAlign.Center)
        .alignItems(HorizontalAlign.Center)
      } else if (this.containers.length === 0) {
        Column({ space: 16 }) {
          Stack() {
            Circle()
              .width(80)
              .height(80)
              .fill(this.themeBgCard)
            Text('☁')
              .fontSize(36)
          }
          .margin({ bottom: 8 })

          Text('暂无远程环境')
            .fontSize(18)
            .fontWeight(FontWeight.Medium)
            .fontColor(this.themeTextPrimary)

          Text('点击右上角 + 新建一个')
            .fontSize(14)
            .fontColor(this.themeTextTertiary)

          Button('新建', { type: ButtonType.Normal })
            .width(160)
            .height(44)
            .borderRadius(22)
            .backgroundColor(this.themeAccent)
            .fontColor('#FFFFFF')
            .fontSize(15)
            .margin({ top: 8 })
            .onClick(() => this.doCreate())
        }
        .width('100%')
        .height('100%')
        .backgroundColor(this.themeBgCard)
        .justifyContent(FlexAlign.Center)
        .alignItems(HorizontalAlign.Center)
      } else {
        List() {
          ForEach(this.containers, (c: ContainerInfo) => {
            ListItem() {
              Column({ space: 0 }) {
                Row() {
                  Stack() {
                    Rect().fill(this.themeAccent).width(48).height(48).borderRadius(8)
                    Text(c.name.charAt(0).toUpperCase())
                      .fontSize(20).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
                  }
                  .margin({ right: 12 })

                  Column({ space: 3 }) {
                    Row({ space: 6 }) {
                      Text(c.name)
                        .fontSize(15)
                        .fontWeight(FontWeight.Medium)
                        .maxLines(1)
                      Text(c.status === 'running' ? '● 运行中' : '○ 已停止')
                        .fontSize(12)
                        .fontColor(c.status === 'running' ? '#4CAF50' : this.themeTextTertiary)
                    }
                    .width('100%')

                    if (c.port) {
                      Text('端口: ' + c.port)
                        .fontSize(13)
                        .fontColor(this.themeTextSecondary)
                    }
                    if (c.opencode_url) {
                      Text(c.opencode_url)
                        .fontSize(13)
                        .fontColor(this.themeConfig)
                        .maxLines(1)
                    }
                  }
                  .layoutWeight(1)
                  .alignItems(HorizontalAlign.Start)
                }
                .width('100%')
                .padding({ left: 16, right: 16, top: 12, bottom: 8 })

                Row({ space: 8 }) {
                  if (c.status === 'stopped' || c.status === 'error') {
                    Button('启动')
                      .height(32)
                      .fontSize(13)
                      .fontColor('#FFFFFF')
                      .backgroundColor(this.themeAccent)
                      .borderRadius(8)
                      .enabled(!this.loading)
                      .layoutWeight(1)
                      .onClick(() => this.doStart(c.id))
                  } else if (c.status === 'running') {
                    Button('停止')
                      .height(32)
                      .fontSize(13)
                      .fontColor('#FFFFFF')
                      .backgroundColor('#FF9800')
                      .borderRadius(8)
                      .enabled(!this.loading)
                      .layoutWeight(1)
                      .onClick(() => this.doStop(c.id))
                  } else {
                    Text('处理中...')
                      .fontSize(13)
                      .fontColor(this.themeTextTertiary)
                      .layoutWeight(1)
                  }

                  Button('添加到后端')
                    .height(32)
                    .fontSize(13)
                    .fontColor('#FFFFFF')
                    .backgroundColor(this.themeConfig)
                    .borderRadius(8)
                    .enabled(!this.loading)
                    .layoutWeight(1)
                    .onClick(() => this.doAddAsBackend(c))

                  Button('删除')
                    .height(32)
                    .fontSize(13)
                    .fontColor('#FFFFFF')
                    .backgroundColor('#FF3B30')
                    .borderRadius(8)
                    .enabled(!this.loading)
                    .layoutWeight(1)
                    .onClick(() => this.doDelete(c.id))
                }
                .width('100%')
                .padding({ left: 16, right: 16, bottom: 12 })
              }
              .width('100%')
              .backgroundColor(this.themeBgCard)
            }
          }, (c: ContainerInfo) => c.id)
        }
        .divider({ strokeWidth: 0.5, color: this.themeBorderPrimary })
        .backgroundColor(this.themeBgCard)
        .width('100%')
        .layoutWeight(1)
      }
    }
    .width('100%')
    .height('100%')
    .backgroundColor(this.themeBgSecondary)
  }
}

@CustomDialog
struct CreateContainerDialog {
  controller: CustomDialogController = new CustomDialogController({ builder: null });
  @State inputValue: string = '';
  onConfirm: (name: string) => void = (_n: string) => {};

  build() {
    Column({ space: 16 }) {
      Text('新建远程环境')
        .fontSize(18)
        .fontWeight(FontWeight.Bold)
        .fontColor('#333333')

      TextInput({ placeholder: '请输入环境名称', text: this.inputValue })
        .width('100%')
        .height(44)
        .fontSize(15)
        .backgroundColor('#F5F5F5')
        .borderRadius(8)
        .onChange((value: string) => {
          this.inputValue = value;
        })

      Row({ space: 12 }) {
        Button('取消')
          .height(40)
          .layoutWeight(1)
          .fontSize(14)
          .fontColor('#666666')
          .backgroundColor('#F0F0F0')
          .borderRadius(8)
          .onClick(() => this.controller.close())

        Button('确定')
          .height(40)
          .layoutWeight(1)
          .fontSize(14)
          .fontColor('#FFFFFF')
          .backgroundColor('#07C160')
          .borderRadius(8)
          .onClick(() => {
            this.onConfirm(this.inputValue);
            this.controller.close();
          })
      }
      .width('100%')
    }
    .width('85%')
    .padding(24)
    .backgroundColor('#FFFFFF')
    .borderRadius(16)
  }
}

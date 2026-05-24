import { OpenCodeCore, ContainerInfo } from '../core/OpenCodeCore';
import router from '@ohos.router';

@Entry
@Component
struct ContainerList {
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  @State containers: ContainerInfo[] = [];
  @State loading: boolean = false;
  @State msg: string = '';
  @StorageLink('themeAccent') themeAccent: string = '#07C160';
  @StorageLink('themeBgPrimary') themeBgPrimary: string = '#F5F5F5';
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
      this.msg = '请输入名称';
      setTimeout(() => { this.msg = ''; }, 2000);
      return;
    }
    this.loading = true;
    try {
      const result = await this.core.createContainer(trimmed);
      if (result.success && result.container) {
        this.containers = [...this.containers, result.container as ContainerInfo];
        this.msg = '创建成功';
        setTimeout(() => { this.msg = ''; }, 2000);
      } else {
        this.msg = result.error || '创建失败';
        setTimeout(() => { this.msg = ''; }, 3000);
      }
    } catch (e) {
      this.msg = '创建失败';
      setTimeout(() => { this.msg = ''; }, 3000);
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
      this.msg = result.error || '加载失败';
      setTimeout(() => { this.msg = ''; }, 3000);
    }
  }

  private async doStart(id: string): Promise<void> {
    this.msg = '';
    const result = await this.core.startContainer(id);
    if (result.success) {
      await this.loadContainers();
    } else {
      this.msg = result.error || '启动失败';
      setTimeout(() => { this.msg = ''; }, 3000);
    }
  }

  private async doStop(id: string): Promise<void> {
    this.msg = '';
    const result = await this.core.stopContainer(id);
    if (result.success) {
      await this.loadContainers();
    } else {
      this.msg = result.error || '停止失败';
      setTimeout(() => { this.msg = ''; }, 3000);
    }
  }

  private async doDelete(id: string): Promise<void> {
    this.msg = '';
    const result = await this.core.deleteContainer(id);
    if (result.success) {
      await this.loadContainers();
    } else {
      this.msg = result.error || '删除失败';
      setTimeout(() => { this.msg = ''; }, 3000);
    }
  }

  private async doAddAsBackend(c: ContainerInfo): Promise<void> {
    if (c.opencode_url && c.opencode_username) {
      await this.core.addBackendFromContainer(c.opencode_url, c.opencode_username, c.opencode_password || '');
      this.msg = '已添加到后端列表';
      setTimeout(() => { this.msg = ''; }, 3000);
    }
  }

  @Builder
  containerItem(c: ContainerInfo) {
    Column({ space: 0 }) {
      Row() {
        Column({ space: 4 }) {
          Row({ space: 8 }) {
            Text(c.name)
              .fontSize(17)
              .fontColor(this.themeTextPrimary)
              .fontWeight(FontWeight.Medium)
            Text(c.status === 'running' ? '● 运行中' : '○ 已停止')
              .fontSize(12)
              .fontColor(c.status === 'running' ? '#4CAF50' : this.themeTextTertiary)
          }
          .width('100%')
          .alignItems(VerticalAlign.Center)

          if (c.port) {
            Text('端口: ' + c.port)
              .fontSize(12)
              .fontColor(this.themeTextSecondary)
          }
          if (c.opencode_url) {
            Text('访问: ' + c.opencode_url)
              .fontSize(12)
              .fontColor(this.themeConfig)
          }
          Text('创建: ' + c.created_at.substring(0, 16).replace('T', ' '))
            .fontSize(12)
            .fontColor(this.themeTextTertiary)
        }
        .alignItems(HorizontalAlign.Start)
        .layoutWeight(1)
      }
      .width('100%')
      .padding(16)

      Divider().height(0.5).color(this.themeBorderSecondary).margin({ left: 16 })

      Row({ space: 8 }) {
        if (c.status === 'stopped' || c.status === 'error') {
          Button('启动')
            .height(32)
            .fontSize(13)
            .fontColor('#FFFFFF')
            .backgroundColor(this.themeAccent)
            .borderRadius(8)
            .enabled(!this.loading)
            .onClick(() => this.doStart(c.id))
        } else if (c.status === 'running') {
          Button('停止')
            .height(32)
            .fontSize(13)
            .fontColor('#FFFFFF')
            .backgroundColor('#FF9800')
            .borderRadius(8)
            .enabled(!this.loading)
            .onClick(() => this.doStop(c.id))
        } else {
          Text('处理中...')
            .fontSize(13)
            .fontColor(this.themeTextTertiary)
        }

        Button('添加')
          .height(32)
          .fontSize(13)
          .fontColor('#FFFFFF')
          .backgroundColor(this.themeConfig)
          .borderRadius(8)
          .enabled(!this.loading)
          .onClick(() => this.doAddAsBackend(c))

        Blank()

        Button('删除')
          .height(32)
          .fontSize(13)
          .fontColor('#FFFFFF')
          .backgroundColor('#FF3B30')
          .borderRadius(8)
          .enabled(!this.loading)
          .onClick(() => this.doDelete(c.id))
      }
      .width('100%')
      .padding(12)
    }
    .width('100%')
    .backgroundColor(this.themeBgCard)
    .borderRadius(12)
    .margin({ top: 8 })
  }

  build() {
    Stack() {
      Column() {
        Column() {
          Row() {
            Text('<')
              .fontSize(22)
              .fontColor(this.themeAccent)
              .padding({ left: 4, right: 12 })
              .onClick(() => router.back())
            Text('远程环境')
              .fontSize(18)
              .fontWeight(FontWeight.Bold)
              .layoutWeight(1)
            Button('新建')
              .height(32)
              .fontSize(14)
              .fontColor('#FFFFFF')
              .backgroundColor(this.themeAccent)
              .borderRadius(8)
              .enabled(!this.loading)
              .onClick(() => this.doCreate())
          }
          .width('100%')
          .padding({ left: 20, right: 20, top: 16, bottom: 12 })

          Divider().height(0.5).color(this.themeBorderPrimary)
        }
        .width('100%')
        .backgroundColor(this.themeBgCard)

        if (this.msg) {
          Text(this.msg)
            .fontSize(13)
            .fontColor(this.msg.includes('已') ? this.themeAccent : '#FF3B30')
            .width('100%')
            .padding(12)
            .textAlign(TextAlign.Center)
        }

        if (this.loading) {
          Text('加载中...')
            .fontSize(14)
            .fontColor(this.themeTextTertiary)
            .width('100%')
            .padding(24)
            .textAlign(TextAlign.Center)
        } else if (this.containers.length === 0) {
          Column({ space: 12 }) {
            Text('暂无远程环境')
              .fontSize(16)
              .fontColor(this.themeTextSecondary)
            Text('点击右上角"新建"创建一个')
              .fontSize(13)
              .fontColor(this.themeTextTertiary)
          }
          .width('100%')
          .padding(40)
          .alignItems(HorizontalAlign.Center)
        } else {
          Scroll() {
            Column({ space: 0 }) {
              ForEach(this.containers, (c: ContainerInfo) => {
                this.containerItem(c)
              })
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 8, bottom: 32 })
          }
          .layoutWeight(1)
        }
      }
      .width('100%')
      .height('100%')
      .backgroundColor(this.themeBgPrimary)
    }
    .width('100%')
    .height('100%')
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

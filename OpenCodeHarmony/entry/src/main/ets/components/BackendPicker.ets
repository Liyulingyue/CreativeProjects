import { OpenCodeBackend } from '../core/OpenCodeCore';

@Component
export struct BackendPicker {
  @Prop backends: OpenCodeBackend[] = [];
  @Prop selectedId: string = '';
  onSelect: (backend: OpenCodeBackend) => void = () => {};
  @StorageLink('themeAccent') themeAccent: string = '#07C160';
  @StorageLink('themeBgCard') themeBgCard: string = '#FFFFFF';
  @StorageLink('themeTextPrimary') themeTextPrimary: string = '#333333';
  @StorageLink('themeBorderPrimary') themeBorderPrimary: string = '#E0E0E0';

  build() {
    Column() {
      ForEach(this.backends, (backend: OpenCodeBackend) => {
        Row() {
          Text(backend.notes || backend.url)
            .fontSize(14)
            .fontColor(this.themeTextPrimary)
            .layoutWeight(1)
            .maxLines(1)
            .textOverflow({ overflow: TextOverflow.Ellipsis })
          if (backend.id === this.selectedId) {
            Text('✓').fontColor(this.themeAccent).fontSize(14)
          }
        }
        .width('100%')
        .height(40)
        .padding({ left: 12, right: 12 })
        .onClick(() => this.onSelect(backend))
      })
    }
    .width('calc(100% - 40vp)')
    .constraintSize({ maxHeight: 200 })
    .backgroundColor(this.themeBgCard)
    .borderRadius(8)
    .border({ width: 1, color: this.themeBorderPrimary })
    .shadow({ radius: 8, color: '#1A000000', offsetX: 0, offsetY: 2 })
    .margin({ top: 145, left: 20 })
    .zIndex(999)
  }
}

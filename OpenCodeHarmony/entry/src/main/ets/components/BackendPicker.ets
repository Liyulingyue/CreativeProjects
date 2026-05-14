import { OpenCodeBackend } from '../core/OpenCodeCore';

@Component
export struct BackendPicker {
  @Prop backends: OpenCodeBackend[] = [];
  @Prop selectedId: string = '';
  onSelect: (backend: OpenCodeBackend) => void = () => {};

  build() {
    Column() {
      ForEach(this.backends, (backend: OpenCodeBackend) => {
        Row() {
          Text(backend.notes || backend.url)
            .fontSize(14)
            .fontColor('#333333')
            .layoutWeight(1)
            .maxLines(1)
            .textOverflow({ overflow: TextOverflow.Ellipsis })
          if (backend.id === this.selectedId) {
            Text('✓').fontColor('#07C160').fontSize(14)
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
    .backgroundColor('#FFFFFF')
    .borderRadius(8)
    .border({ width: 1, color: '#E0E0E0' })
    .shadow({ radius: 8, color: '#1A000000', offsetX: 0, offsetY: 2 })
    .margin({ top: 145, left: 20 })
    .zIndex(999)
  }
}

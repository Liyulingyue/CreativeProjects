interface FileNode {
  name: string;
  path: string;
  absolute: string;
  type: 'file' | 'directory';
  ignored: boolean;
}

@Component
export struct PathAutocomplete {
  @Prop recentPaths: string[] = [];
  @Prop searchResults: FileNode[] = [];
  @Prop isSearching: boolean = false;
  @Link showPicker: boolean;
  onPathSelect: (path: string) => void = () => {};
  onRecentSelect: (path: string) => void = () => {};

  build() {
    Column() {
      if (this.recentPaths.length > 0) {
        Text('最近使用')
          .fontSize(12)
          .fontColor('#999999')
          .fontWeight(FontWeight.Medium)
          .width('100%')
          .padding({ left: 12, right: 12, top: 10, bottom: 4 })
        ForEach(this.recentPaths.slice(0, 5), (path: string) => {
          Row() {
            Text(path)
              .fontSize(13)
              .fontColor('#333333')
              .maxLines(1)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
              .layoutWeight(1)
            Text('最近')
              .fontSize(10)
              .fontColor('#07C160')
              .padding({ left: 6 })
          }
          .width('100%')
          .height(36)
          .padding({ left: 12, right: 12 })
          .onClick(() => {
            this.onRecentSelect(path);
          })
        })
        Divider().height(0.5).color('#EEEEEE').margin({ top: 4, bottom: 4 })
      }

      if (this.isSearching) {
        Row() {
          LoadingProgress().width(14).height(14)
          Text('搜索中...').fontSize(12).fontColor('#999999').margin({ left: 6 })
        }
        .width('100%')
        .height(32)
        .padding({ left: 12 })
      } else if (this.searchResults.length > 0) {
        ForEach(this.searchResults.slice(0, 8), (item: FileNode) => {
          Row() {
            Text('📁')
              .fontSize(12)
              .margin({ right: 6 })
            Text(item.absolute)
              .fontSize(13)
              .fontColor('#333333')
              .maxLines(1)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
              .layoutWeight(1)
          }
          .width('100%')
          .height(36)
          .padding({ left: 12, right: 12 })
          .onClick(() => {
            this.onPathSelect(item.absolute);
          })
        })
      }
    }
    .width('calc(100% - 40vp)')
    .constraintSize({ maxHeight: 300 })
    .backgroundColor('#FFFFFF')
    .borderRadius(8)
    .border({ width: 1, color: '#E0E0E0' })
    .shadow({ radius: 8, color: '#1A000000', offsetX: 0, offsetY: 2 })
    .margin({ top: 230, left: 20 })
    .zIndex(999)
  }
}

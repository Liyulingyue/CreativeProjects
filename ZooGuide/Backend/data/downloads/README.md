# Downloads

此目录存放供用户下载的资料文件（宣传册、折页、讲解稿等）。

文件由 `meta.downloads` 配置（在 `venues.json` 中）决定展示列表，
后端通过 `/downloads/{filename}` 路由提供静态文件服务。

`.gitkeep` 和 `README.md` 不会被列入下载列表。

## 部署时

将实际文件放入此目录即可，例如：

```
Backend/data/downloads/
├── 宣传册中文版.pdf
├── 折页中文版.pdf
├── 折页英文版.pdf
├── 2025南京市红山森林动物园讲解稿更新.docx
└── 推文内容素材池整理.docx
```

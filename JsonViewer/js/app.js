/**
 * JSON Tree Viewer - 主应用模块
 * 负责初始化、协调各个模块、处理用户交互
 */

class JsonTreeViewerApp {
    constructor() {
        this.editor = null;
        this.treeRenderer = null;
        this.currentJson = null;
    }

    /**
     * 初始化应用
     */
    init() {
        this.initEditor();
        this.initTreeRenderer();
        this.bindEvents();
        console.log('✅ JSON Tree Viewer 已初始化');
    }

    /**
     * 初始化 JSON 编辑器
     */
    initEditor() {
        const container = document.getElementById('json-editor');
        if (!container) {
            console.error('❌ 未找到 JSON 编辑器容器');
            return;
        }

        const options = {
            mode: 'code',
            modes: ['code', 'tree', 'view', 'form'],
            onChange: () => this.handleJsonChange(),
            onError: (error) => this.handleError(error)
        };

        try {
            this.editor = new JSONEditor(container, options);
        } catch (e) {
            console.error('❌ JSONEditor 初始化失败:', e);
        }
    }

    /**
     * 初始化树形渲染器
     */
    initTreeRenderer() {
        const container = document.getElementById('tree-view');
        if (!container) {
            console.error('❌ 未找到树形视图容器');
            return;
        }

        this.treeRenderer = new TreeRenderer(container);
    }

    /**
     * 绑定事件监听
     */
    bindEvents() {
        // 键盘快捷键
        document.addEventListener('keydown', (e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
                e.preventDefault();
                this.handleJsonChange();
            }
        });

        // 文件上传
        const fileInput = document.getElementById('file-input');
        if (fileInput) {
            fileInput.addEventListener('change', (e) => this.handleFileUpload(e));
        }
    }

    /**
     * 处理文件上传
     * @param {Event} event - 文件选择事件
     */
    handleFileUpload(event) {
        const file = event.target.files?.[0];
        if (!file) return;

        // 检查文件类型
        const validExtensions = ['.json', '.jsonl', '.txt'];
        const fileName = file.name.toLowerCase();
        const isValid = validExtensions.some(ext => fileName.endsWith(ext));

        if (!isValid) {
            this.showError('请上传 .json、.jsonl 或 .txt 文件');
            return;
        }

        // 判断是否为 JSONL 文件
        const isJsonl = fileName.endsWith('.jsonl');

        // 检查文件大小 (限制 5MB)
        const maxSize = 5 * 1024 * 1024;
        if (file.size > maxSize) {
            this.showError('文件大小不能超过 5MB');
            return;
        }

        const reader = new FileReader();

        reader.onload = (e) => {
            try {
                const content = e.target.result;
                
                if (isJsonl) {
                    // 解析 JSONL 格式
                    const result = JsonlParser.parse(content);
                    
                    if (!result.success) {
                        this.showError(result.error);
                        return;
                    }
                    
                    // 使用特殊方式处理 JSONL
                    this.loadJsonlData(result);
                    
                    // 如果有解析警告，显示部分错误
                    if (result.hasWarnings) {
                        console.warn(`⚠️ JSONL 解析完成，但有 ${result.errors.length} 行解析失败`);
                    }
                    
                    console.log(`✅ 已加载 JSONL 文件: ${file.name} (${result.lineCount} 条记录)`);
                } else {
                    // 普通 JSON 文件
                    const json = JSON.parse(content);
                    this.editor.set(json);
                    this.handleJsonChange();
                    console.log(`✅ 已加载文件: ${file.name}`);
                }
            } catch (error) {
                this.showError(`文件解析失败: ${error.message}`);
            }
        };

        reader.onerror = () => {
            this.showError('文件读取失败');
        };

        reader.readAsText(file);

        // 清空 input，允许重复选择同一文件
        event.target.value = '';
    }

    /**
     * 处理 JSON 变化
     */
    handleJsonChange() {
        const errorPanel = document.getElementById('error-panel');
        
        try {
            const json = this.editor.get();
            this.currentJson = json;
            this.hideError();
            this.treeRenderer.render(json);
        } catch (e) {
            this.showError(e.message);
        }
    }

    /**
     * 处理错误
     * @param {Error} error
     */
    handleError(error) {
        this.showError(error.message || 'JSON 解析错误');
    }

    /**
     * 显示错误信息
     * @param {string} message
     */
    showError(message) {
        const errorPanel = document.getElementById('error-panel');
        if (errorPanel) {
            errorPanel.textContent = `❌ 错误: ${message}`;
            errorPanel.classList.add('show');
        }
    }

    /**
     * 隐藏错误信息
     */
    hideError() {
        const errorPanel = document.getElementById('error-panel');
        if (errorPanel) {
            errorPanel.classList.remove('show');
        }
    }

    /**
     * 切换节点展开/折叠状态
     * @param {string} id - 节点 ID
     */
    toggleNode(id) {
        const children = document.getElementById(id);
        if (!children) return;

        const toggle = children.previousElementSibling?.previousElementSibling;
        
        if (children.classList.contains('collapsed')) {
            children.classList.remove('collapsed');
            if (toggle?.classList.contains('tree-toggle')) {
                toggle.textContent = '▼';
            }
        } else {
            children.classList.add('collapsed');
            if (toggle?.classList.contains('tree-toggle')) {
                toggle.textContent = '▶';
            }
        }
    }

    /**
     * 展开所有节点
     */
    expandAll() {
        this.treeRenderer.expandAll();
    }

    /**
     * 折叠所有节点
     */
    collapseAll() {
        this.treeRenderer.collapseAll();
    }

    /**
     * 加载示例数据
     * @param {string} sampleKey - 示例数据的键名
     */
    loadSample(sampleKey = 'default') {
        const sample = SampleData.getByKey(sampleKey);
        if (sample) {
            this.editor.set(sample);
            this.handleJsonChange();
        }
    }

    /**
     * 加载所有示例数据（打开选择菜单）
     */
    loadSampleMenu() {
        const samples = SampleData.getAllSamples();
        const menu = document.getElementById('sample-menu');
        
        if (!menu) {
            this.createSampleMenu(samples);
            return;
        }

        menu.classList.toggle('hidden');
    }

    /**
     * 创建示例菜单
     * @param {Array} samples
     */
    createSampleMenu(samples) {
        const panelHeader = document.querySelector('.panel:first-child .panel-header');
        if (!panelHeader) return;

        // 创建菜单容器
        const menu = document.createElement('div');
        menu.id = 'sample-menu';
        menu.className = 'sample-menu';
        menu.innerHTML = `
            <style>
                .sample-menu {
                    position: absolute;
                    top: 100%;
                    right: 20px;
                    background: white;
                    border-radius: 8px;
                    box-shadow: 0 4px 20px rgba(0,0,0,0.15);
                    z-index: 100;
                    min-width: 150px;
                    overflow: hidden;
                }
                .sample-menu.hidden { display: none; }
                .sample-menu-item {
                    padding: 10px 16px;
                    cursor: pointer;
                    color: #333;
                    transition: background 0.2s;
                }
                .sample-menu-item:hover {
                    background: #f5f5f5;
                }
            </style>
        `;

        samples.forEach(sample => {
            const item = document.createElement('div');
            item.className = 'sample-menu-item';
            item.textContent = sample.name;
            item.onclick = () => {
                this.loadSample(sample.key);
                menu.classList.add('hidden');
            };
            menu.appendChild(item);
        });

        // 点击其他地方关闭菜单
        document.addEventListener('click', (e) => {
            if (!menu.contains(e.target) && !e.target.closest('.btn')) {
                menu.classList.add('hidden');
            }
        });

        panelHeader.style.position = 'relative';
        panelHeader.appendChild(menu);
    }

    /**
     * 清空编辑器
     */
    clearEditor() {
        this.editor.set({});
        this.hideError();
        this.treeRenderer.render(null);
    }

    /**
     * 获取当前 JSON 数据
     * @returns {*}
     */
    getCurrentJson() {
        return this.currentJson;
    }

    /**
     * 设置 JSON 数据
     * @param {*} json
     */
    setJson(json) {
        this.editor.set(json);
        this.handleJsonChange();
    }

    /**
     * 加载 JSONL 数据
     * @param {{ data: Array, lineCount: number, hasWarnings?: boolean }} result - JSONL 解析结果
     */
    loadJsonlData(result) {
        this.currentJson = result.data;
        this.isJsonlMode = true;
        this.hideError();
        
        // 直接渲染 JSONL 树形视图
        const container = document.getElementById('tree-view');
        if (container) {
            container.innerHTML = JsonlParser.formatAsTree(result.data);
        }
        
        // 清空编辑器（JSONL 不适合在代码编辑器中显示）
        if (this.editor) {
            this.editor.set({ __jsonl_placeholder: `JSONL 模式: ${result.lineCount} 条记录` });
        }
    }

    /**
     * 判断当前是否处于 JSONL 模式
     * @returns {boolean}
     */
    isJsonl() {
        return this.isJsonlMode === true;
    }

    /**
     * 退出 JSONL 模式
     */
    exitJsonlMode() {
        this.isJsonlMode = false;
        this.treeRenderer.render(null);
    }
}

// 创建全局应用实例
let app = null;

// 页面加载完成后初始化
document.addEventListener('DOMContentLoaded', () => {
    app = new JsonTreeViewerApp();
    app.init();
});

// 导出到全局
window.JsonTreeViewerApp = JsonTreeViewerApp;
window.app = app;

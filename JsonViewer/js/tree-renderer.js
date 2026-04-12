/**
 * TreeRenderer - 树形视图渲染器
 * 负责将 JSON 对象递归渲染为树形 HTML 结构
 */

class TreeRenderer {
    constructor(container) {
        this.container = container;
        this.nodeCounter = 0;
    }

    /**
     * 渲染整个树
     * @param {*} json - JSON 数据
     */
    render(json) {
        this.nodeCounter = 0;
        
        if (json === null || json === undefined) {
            this.container.innerHTML = this.renderEmptyState('请输入有效的 JSON 数据');
            return;
        }

        if (typeof json === 'object') {
            const html = this.renderNode(json, 'root', true);
            this.container.innerHTML = html;
        } else {
            this.container.innerHTML = this.renderPrimitive(json, null, true);
        }
    }

    /**
     * 渲染单个节点
     * @param {*} node - 节点数据
     * @param {string|number} key - 键名
     * @param {boolean} isRoot - 是否为根节点
     * @returns {string} HTML 字符串
     */
    renderNode(node, key, isRoot = false) {
        if (Array.isArray(node)) {
            return this.renderArray(node, key, isRoot);
        } else if (typeof node === 'object' && node !== null) {
            return this.renderObject(node, key, isRoot);
        } else {
            return this.renderPrimitive(node, key, isRoot);
        }
    }

    /**
     * 渲染数组
     * @param {Array} arr - 数组数据
     * @param {string|number} key - 键名
     * @param {boolean} isRoot - 是否为根节点
     * @returns {string} HTML 字符串
     */
    renderArray(arr, key, isRoot) {
        const id = this.generateNodeId();
        const count = arr.length;
        const keyHtml = isRoot ? '' : `<span class="tree-key">"${this.escapeHtml(String(key))}"</span>: `;
        const toggle = this.renderToggle(id);

        let childrenHtml = '';
        arr.forEach((item, index) => {
            childrenHtml += `<div class="tree-node"><div class="tree-item">${this.renderNode(item, index)}</div></div>`;
        });

        if (arr.length === 0) {
            return `${keyHtml}${toggle}<span class="tree-bracket">[ ]</span>`;
        }

        return `${keyHtml}${toggle}<span class="tree-bracket">[</span><span class="tree-count">${count} 项</span>
            <div class="tree-children" id="${id}">${childrenHtml}</div>
            <span class="tree-bracket">]</span>`;
    }

    /**
     * 渲染对象
     * @param {Object} obj - 对象数据
     * @param {string|number} key - 键名
     * @param {boolean} isRoot - 是否为根节点
     * @returns {string} HTML 字符串
     */
    renderObject(obj, key, isRoot) {
        const id = this.generateNodeId();
        const keys = Object.keys(obj);
        const count = keys.length;
        const keyHtml = isRoot ? '' : `<span class="tree-key">"${this.escapeHtml(String(key))}"</span>: `;
        const toggle = this.renderToggle(id);

        let childrenHtml = '';
        keys.forEach(k => {
            childrenHtml += `<div class="tree-node"><div class="tree-item">${this.renderNode(obj[k], k)}</div></div>`;
        });

        if (keys.length === 0) {
            return `${keyHtml}${toggle}<span class="tree-bracket">{ }</span>`;
        }

        return `${keyHtml}${toggle}<span class="tree-bracket">{</span><span class="tree-count">${count} 个属性</span>
            <div class="tree-children" id="${id}">${childrenHtml}</div>
            <span class="tree-bracket">}</span>`;
    }

    /**
     * 渲染原始类型值
     * @param {*} value - 值
     * @param {string|number} key - 键名
     * @param {boolean} isRoot - 是否为根节点
     * @returns {string} HTML 字符串
     */
    renderPrimitive(value, key, isRoot) {
        const keyHtml = isRoot ? '' : `<span class="tree-key">"${this.escapeHtml(String(key))}"</span>: `;
        const type = this.getValueType(value);

        if (value === null) {
            return `${keyHtml}<span class="tree-value null">null</span>`;
        }
        if (value === undefined) {
            return `${keyHtml}<span class="tree-value null">undefined</span>`;
        }

        return `${keyHtml}<span class="tree-value ${type}">${this.formatValue(value, type)}</span>`;
    }

    /**
     * 渲染展开/折叠按钮
     * @param {string} id - 节点 ID
     * @returns {string} HTML 字符串
     */
    renderToggle(id) {
        return `<span class="tree-toggle" onclick="app.toggleNode('${id}')" title="点击展开/折叠">▼</span>`;
    }

    /**
     * 生成唯一节点 ID
     * @returns {string}
     */
    generateNodeId() {
        return `node-${++this.nodeCounter}-${Date.now()}`;
    }

    /**
     * 获取值类型
     * @param {*} value
     * @returns {string}
     */
    getValueType(value) {
        if (value === null) return 'null';
        if (typeof value === 'string') return 'string';
        if (typeof value === 'number') return 'number';
        if (typeof value === 'boolean') return 'boolean';
        return 'object';
    }

    /**
     * 格式化值显示
     * @param {*} value
     * @param {string} type
     * @returns {string}
     */
    formatValue(value, type) {
        if (type === 'string') return `"${this.escapeHtml(value)}"`;
        if (type === 'null') return 'null';
        return String(value);
    }

    /**
     * HTML 转义，防止 XSS
     * @param {string} text
     * @returns {string}
     */
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    /**
     * 渲染空状态
     * @param {string} message
     * @returns {string}
     */
    renderEmptyState(message) {
        return `
            <div class="empty-state">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                    <path d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
                </svg>
                <p>${message}</p>
            </div>
        `;
    }

    /**
     * 展开所有节点
     */
    expandAll() {
        this.container.querySelectorAll('.tree-children.collapsed').forEach(el => {
            el.classList.remove('collapsed');
        });
        this.container.querySelectorAll('.tree-toggle').forEach(el => {
            el.textContent = '▼';
        });
    }

    /**
     * 折叠所有节点
     */
    collapseAll() {
        this.container.querySelectorAll('.tree-children').forEach(el => {
            el.classList.add('collapsed');
        });
        this.container.querySelectorAll('.tree-toggle').forEach(el => {
            el.textContent = '▶';
        });
    }
}

// 导出到全局
window.TreeRenderer = TreeRenderer;

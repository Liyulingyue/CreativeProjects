/**
 * JSONL Parser - JSON Lines 格式解析器
 * 
 * JSONL 格式：每行是一个独立的 JSON 对象，用换行符分隔
 * 示例：
 *   {"name": "Alice", "age": 30}
 *   {"name": "Bob", "age": 25}
 *   {"name": "Charlie", "age": 35}
 */

const JsonlParser = {
    /**
     * 解析 JSONL 文本
     * @param {string} text - JSONL 文本内容
     * @returns {{ success: boolean, data?: Array, error?: string, lineCount?: number }}
     */
    parse(text) {
        if (!text || typeof text !== 'string') {
            return { success: false, error: '输入内容为空' };
        }

        const lines = text.trim().split('\n');
        const results = [];
        const errors = [];

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i].trim();
            
            // 跳过空行
            if (!line) continue;

            try {
                const parsed = JSON.parse(line);
                results.push(parsed);
            } catch (e) {
                errors.push({
                    line: i + 1,
                    content: line.substring(0, 50) + (line.length > 50 ? '...' : ''),
                    error: e.message
                });
            }
        }

        if (errors.length > 0 && results.length === 0) {
            return {
                success: false,
                error: `第 ${errors[0].line} 行解析失败: ${errors[0].error}`,
                errors: errors
            };
        }

        return {
            success: true,
            data: results,
            lineCount: results.length,
            errors: errors.length > 0 ? errors : null,
            hasWarnings: errors.length > 0
        };
    },

    /**
     * 检测文本是否为 JSONL 格式
     * @param {string} text
     * @returns {boolean}
     */
    isJsonl(text) {
        if (!text || typeof text !== 'string') return false;
        
        const lines = text.trim().split('\n').filter(l => l.trim());
        
        // 至少需要 2 行且每行都是独立 JSON 对象
        if (lines.length < 2) return false;
        
        let jsonlCount = 0;
        let jsonCount = 0;

        for (const line of lines) {
            try {
                const parsed = JSON.parse(line);
                // JSONL 的特征：每行是一个独立的对象（不是数组）
                if (typeof parsed === 'object' && !Array.isArray(parsed)) {
                    jsonlCount++;
                }
            } catch {
                // 不是有效 JSON
                return false;
            }
        }

        // 如果大多数行都是独立对象，认为是 JSONL
        return jsonlCount >= 2;
    },

    /**
     * 将 JSONL 转换为普通 JSON 数组
     * @param {Array} data - JSONL 解析出的数据数组
     * @returns {object}
     */
    toJsonArray(data) {
        return { __jsonl_root: true, items: data };
    },

    /**
     * 格式化 JSONL 为带行号的展示格式
     * @param {Array} data - JSONL 解析出的数据数组
     * @returns {string} HTML
     */
    formatAsTree(data) {
        let html = '<div class="jsonl-container">';
        html += `<div class="jsonl-header"><span class="tree-bracket">[</span> <span class="tree-count">${data.length} 条记录</span></div>`;
        
        data.forEach((item, index) => {
            const lineId = `jsonl-line-${index}`;
            html += `<div class="tree-node jsonl-line">`;
            html += `<div class="tree-item">`;
            html += `<span class="jsonl-line-number">${index + 1}</span>`;
            html += `<span class="tree-toggle" onclick="app.toggleNode('${lineId}')">▼</span>`;
            html += `<span class="tree-bracket">{</span>`;
            
            const keys = Object.keys(item);
            if (keys.length > 0) {
                html += `<span class="tree-count">${keys.length} 个属性</span>`;
                html += `<div class="tree-children" id="${lineId}">`;
                keys.forEach(key => {
                    html += `<div class="tree-node"><div class="tree-item">`;
                    html += `<span class="tree-key">"${this.escapeHtml(key)}"</span>: `;
                    html += this.formatValue(item[key]);
                    html += `</div></div>`;
                });
                html += `</div>`;
            }
            
            html += `<span class="tree-bracket">}</span>`;
            html += `</div></div>`;
        });
        
        html += `<div class="jsonl-footer"><span class="tree-bracket">]</span></div>`;
        html += '</div>';
        
        return html;
    },

    /**
     * 格式化单个值
     * @param {*} value
     * @returns {string}
     */
    formatValue(value) {
        if (value === null) return '<span class="tree-value null">null</span>';
        if (typeof value === 'string') return `<span class="tree-value string">"${this.escapeHtml(value)}"</span>`;
        if (typeof value === 'number') return `<span class="tree-value number">${value}</span>`;
        if (typeof value === 'boolean') return `<span class="tree-value boolean">${value}</span>`;
        if (Array.isArray(value)) {
            let html = '<span class="tree-bracket">[</span>';
            html += `<span class="tree-count">${value.length} 项</span>`;
            if (value.length > 0) {
                html += '<div class="tree-children">';
                value.forEach((item, i) => {
                    html += `<div class="tree-node"><div class="tree-item">${this.formatValue(item)}</div></div>`;
                });
                html += '</div>';
            }
            html += '<span class="tree-bracket">]</span>';
            return html;
        }
        if (typeof value === 'object') {
            let html = '<span class="tree-bracket">{</span>';
            const keys = Object.keys(value);
            html += `<span class="tree-count">${keys.length} 个属性</span>`;
            if (keys.length > 0) {
                html += '<div class="tree-children">';
                keys.forEach(key => {
                    html += `<div class="tree-node"><div class="tree-item">`;
                    html += `<span class="tree-key">"${this.escapeHtml(key)}"</span>: `;
                    html += this.formatValue(value[key]);
                    html += `</div></div>`;
                });
                html += '</div>';
            }
            html += '<span class="tree-bracket">}</span>';
            return html;
        }
        return String(value);
    },

    /**
     * HTML 转义
     * @param {string} text
     * @returns {string}
     */
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
};

// 导出到全局
window.JsonlParser = JsonlParser;

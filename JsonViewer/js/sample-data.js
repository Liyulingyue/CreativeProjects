/**
 * SampleData - 示例数据模块
 * 提供用于演示的 JSON 数据
 */

const SampleData = {
    /**
     * 默认示例：JSON 树形预览器项目数据
     */
    default: {
        "项目名称": "JSON 树形预览器",
        "版本": "1.0.0",
        "特性": {
            "核心功能": ["JSON 解析", "树形展示", "展开/折叠"],
            "交互特性": ["语法高亮", "错误提示", "实时预览"],
            "免依赖": true
        },
        "作者": {
            "姓名": "MRA Agent",
            "邮箱": "agent@mra-system.ai",
            "项目地址": "workspace/Agent-03-planer/web_tools/json_viewer"
        },
        "统计数据": {
            "功能数": 6,
            "测试通过": true,
            "覆盖率": 0.95
        },
        "配置": null,
        "标签": ["JSON", "可视化", "预览器", "纯前端"]
    },

    /**
     * 复杂嵌套示例
     */
    nested: {
        "公司信息": {
            "名称": "示例科技",
            "成立时间": "2020-01-15",
            "办公室": [
                {
                    "位置": "北京",
                    "地址": "朝阳区建国路88号",
                    "员工数": 150,
                    "部门": ["研发", "产品", "运营"]
                },
                {
                    "位置": "上海",
                    "地址": "浦东新区世纪大道1000号",
                    "员工数": 80,
                    "部门": ["销售", "市场"]
                }
            ],
            "财务": {
                "2023年": {
                    "收入": 50000000,
                    "支出": 35000000,
                    "净利润": 15000000
                },
                "2024年": {
                    "收入": 80000000,
                    "支出": 50000000,
                    "净利润": 30000000
                }
            },
            "创始人": ["张三", "李四", "王五"],
            "已上市": false
        }
    },

    /**
     * 数组示例
     */
    arrays: {
        "数字数组": [1, 2, 3, 4, 5],
        "字符串数组": ["苹果", "香蕉", "橙子", "葡萄"],
        "混合数组": [1, "two", true, null, {"nested": "object"}],
        "空数组": [],
        "嵌套数组": [[1, 2], [3, 4], [5, 6]]
    },

    /**
     * 错误示例（用于测试错误提示）
     */
    getInvalidJson() {
        return '{ "name": "test", }'; // 尾部逗号是无效的
    },

    /**
     * 获取所有示例列表
     * @returns {Array<{name: string, data: object}>}
     */
    getAllSamples() {
        return [
            { name: '默认示例', key: 'default', data: this.default },
            { name: '复杂嵌套', key: 'nested', data: this.nested },
            { name: '数组示例', key: 'arrays', data: this.arrays }
        ];
    },

    /**
     * 根据 key 获取示例
     * @param {string} key
     * @returns {object|null}
     */
    getByKey(key) {
        return this[key] || null;
    }
};

// 导出到全局
window.SampleData = SampleData;

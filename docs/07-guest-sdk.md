# Guest SDK

## 定位

Guest SDK 是面向 WASM 模块开发者的依赖库，将底层 WIT 绑定封装为友好的 Rust API，屏蔽 Host/Guest 交互细节。

## 组成

| crate | 编译目标 | 职责 |
|-------|---------|------|
| sdk | wasm32-wasip2 | 封装 WIT 绑定、提供能力 API、MachineBuilder、Action trait |
| sdk-macros | native (proc_macro) | 过程宏，简化状态机和 Action 声明 |

## 分层

```
用户代码
  ↓ 使用
sdk (公开 API: 能力模块、Builder、trait、宏)
  ↓ 内部
WIT 绑定 (由 wit-bindgen 生成，对用户不可见)
  ↓ 运行时
Host 提供的能力实现
```

## 设计要点

- WIT 绑定作为 SDK 内部实现细节，不暴露给用户
- 能力 API 按模块组织（http、kv、log），用户按需导入
- 过程宏降低样板代码，但不引入魔法——展开后的代码用户能看懂
- SDK 版本与 WIT 契约版本对齐

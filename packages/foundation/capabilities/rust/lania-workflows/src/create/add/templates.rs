//! 生成器产物模板（§4.5.3）：一个生成器产出 N 个文件，每个文件对应一个模板。
//!
//! 模板是**代码骨架**而不是空占位：框架约定（Nest 的 `@Injectable()` / DI 注入、
//! DTO 校验装饰器）写死在模板里，这样生成结果可以直接跑起来，
//! 这也是「按框架注册生成器」与「通用模板」的根本区别。
//!
//! 占位符与 `names::NameVariants::placeholders` 一致：
//! `{name}` `{Name}` `{name_kebab}` `{name_snake}` `{name_pascal}` `{name_camel}`
//! `{name_singular}` `{NameSingular}`，另有 `{cssProcessor}` / `{language}`。

/// 一条模板定义。
#[derive(Debug, Clone, Copy)]
pub struct ArtifactTemplate {
    pub name: &'static str,
    pub content: &'static str,
}

const NESTJS_MODULE: &str = "\
import { Module } from '@nestjs/common';

import { {Name}Controller } from './{name_kebab}.controller';
import { {Name}Service } from './{name_kebab}.service';

@Module({
  controllers: [{Name}Controller],
  providers: [{Name}Service],
  exports: [{Name}Service],
})
export class {Name}Module {}
";

const NESTJS_CONTROLLER: &str = "\
import { Body, Controller, Delete, Get, Param, Patch, Post } from '@nestjs/common';

import { Create{NameSingular}Dto } from './dto/create-{name_singular}.dto';
import { Update{NameSingular}Dto } from './dto/update-{name_singular}.dto';
import { {Name}Service } from './{name_kebab}.service';

@Controller('{name_kebab}')
export class {Name}Controller {
  constructor(private readonly {name_camel}Service: {Name}Service) {}

  @Post()
  create(@Body() dto: Create{NameSingular}Dto) {
    return this.{name_camel}Service.create(dto);
  }

  @Get()
  findAll() {
    return this.{name_camel}Service.findAll();
  }

  @Get(':id')
  findOne(@Param('id') id: string) {
    return this.{name_camel}Service.findOne(id);
  }

  @Patch(':id')
  update(@Param('id') id: string, @Body() dto: Update{NameSingular}Dto) {
    return this.{name_camel}Service.update(id, dto);
  }

  @Delete(':id')
  remove(@Param('id') id: string) {
    return this.{name_camel}Service.remove(id);
  }
}
";

const NESTJS_SERVICE: &str = "\
import { Injectable, NotFoundException } from '@nestjs/common';

import { Create{NameSingular}Dto } from './dto/create-{name_singular}.dto';
import { Update{NameSingular}Dto } from './dto/update-{name_singular}.dto';

export interface {NameSingular} {
  id: string;
}

@Injectable()
export class {Name}Service {
  private readonly items = new Map<string, {NameSingular}>();

  create(dto: Create{NameSingular}Dto): {NameSingular} {
    const item: {NameSingular} = { id: crypto.randomUUID(), ...dto };
    this.items.set(item.id, item);
    return item;
  }

  findAll(): {NameSingular}[] {
    return [...this.items.values()];
  }

  findOne(id: string): {NameSingular} {
    const item = this.items.get(id);
    if (!item) {
      throw new NotFoundException(`{NameSingular} ${id} not found`);
    }
    return item;
  }

  update(id: string, dto: Update{NameSingular}Dto): {NameSingular} {
    const item = this.findOne(id);
    const next = { ...item, ...dto };
    this.items.set(id, next);
    return next;
  }

  remove(id: string): { id: string } {
    this.findOne(id);
    this.items.delete(id);
    return { id };
  }
}
";

const NESTJS_DTO_CREATE: &str = "\
import { IsNotEmpty, IsOptional, IsString } from 'class-validator';

export class Create{NameSingular}Dto {
  @IsString()
  @IsNotEmpty()
  name!: string;

  @IsOptional()
  @IsString()
  description?: string;
}
";

const NESTJS_DTO_UPDATE: &str = "\
import { PartialType } from '@nestjs/mapped-types';

import { Create{NameSingular}Dto } from './create-{name_singular}.dto';

export class Update{NameSingular}Dto extends PartialType(Create{NameSingular}Dto) {}
";

const NESTJS_ENTITY: &str = "\
export class {NameSingular}Entity {
  id!: string;
  name!: string;
  description?: string;
  createdAt!: Date;
  updatedAt!: Date;
}
";

const NESTJS_CONTROLLER_SPEC: &str = "\
import { Test } from '@nestjs/testing';

import { {Name}Controller } from './{name_kebab}.controller';
import { {Name}Service } from './{name_kebab}.service';

describe('{Name}Controller', () => {
  let controller: {Name}Controller;

  beforeEach(async () => {
    const moduleRef = await Test.createTestingModule({
      controllers: [{Name}Controller],
      providers: [{Name}Service],
    }).compile();

    controller = moduleRef.get({Name}Controller);
  });

  it('is defined', () => {
    expect(controller).toBeDefined();
  });
});
";

const NESTJS_RESOLVER: &str = "\
import { Args, Mutation, Query, Resolver } from '@nestjs/graphql';

import { Create{NameSingular}Dto } from './dto/create-{name_singular}.dto';
import { {Name}Service } from './{name_kebab}.service';

@Resolver()
export class {Name}Resolver {
  constructor(private readonly {name_camel}Service: {Name}Service) {}

  @Query(() => [String])
  {name_camel}All() {
    return this.{name_camel}Service.findAll();
  }

  @Mutation(() => String)
  create{NameSingular}(@Args('input') input: Create{NameSingular}Dto) {
    return this.{name_camel}Service.create(input);
  }
}
";

const NESTJS_GUARD: &str = "\
import { CanActivate, ExecutionContext, Injectable } from '@nestjs/common';

@Injectable()
export class {Name}Guard implements CanActivate {
  canActivate(context: ExecutionContext): boolean {
    const request = context.switchToHttp().getRequest();
    return Boolean(request?.user);
  }
}
";

const NESTJS_SINGLE: &str = "\
import { Injectable } from '@nestjs/common';

@Injectable()
export class {Name} {}
";

const COMPONENT_RFC: &str = "\
export interface {Name}Props {
  className?: string;
}

export function {Name}(props: {Name}Props) {
  return <div className={props.className} />;
}

export default {Name};
";

const COMPONENT_RCC: &str = "\
import { Component } from 'react';

import type { ReactNode } from 'react';

export interface {Name}Props {
  children?: ReactNode;
}

export class {Name} extends Component<{Name}Props> {
  override render() {
    return <div>{this.props.children}</div>;
  }
}

export default {Name};
";

const COMPONENT_V2: &str = "\
<template>
  <div></div>
</template>

<script>
export default {
  name: '{Name}',
};
</script>
";

const COMPONENT_V3: &str = "\
<template>
  <div></div>
</template>

<script setup lang=\"typescript\">
</script>
";

const COMPONENT_SVELTE: &str = "\
<script lang=\"ts\">
  let className = '';
</script>

<div class={className}></div>
";

const COMPONENT_ASTRO: &str = "\
---
interface Props {
  className?: string;
}

const { className = '' } = Astro.props;
---

<div class={className}></div>
";

const CONFIG_ESLINT: &str = "\
export default [
  {
    files: ['**/*.{js,jsx,ts,tsx}'],
    rules: {},
  },
];
";

const CONFIG_PRETTIER: &str = "\
/** @type {import('prettier').Config} */
export default {
  singleQuote: true,
  printWidth: 100,
};
";

const CONFIG_STYLELINT: &str = "\
/** @type {import('stylelint').Config} */
export default {
  extends: ['stylelint-config-standard'],
  rules: {},
};
";

const CONFIG_TSCONFIG: &str = "\
{
  \"compilerOptions\": {
    \"target\": \"ES2022\",
    \"module\": \"ESNext\",
    \"moduleResolution\": \"Bundler\",
    \"strict\": true,
    \"skipLibCheck\": true
  },
  \"include\": [\"src\"]
}
";

const CONFIG_EDITORCONFIG: &str = "\
root = true

[*]
charset = utf-8
end_of_line = lf
indent_style = space
indent_size = 2
insert_final_newline = true
trim_trailing_whitespace = true
";

const CONFIG_GITIGNORE: &str = "\
node_modules
dist
coverage
*.log
";

const CONFIG_COMMITIZEN: &str = "\
export default {
  path: 'cz-conventional-changelog',
};
";

// ---------------------------------------------------------------------------
// B7 框架模板：nextjs / gin / fastapi / axum / flutter
// ---------------------------------------------------------------------------

const NEXTJS_PAGE: &str = "\
export const metadata = {
  title: '{Name}',
};

export default function {Name}Page() {
  return (
    <main>
      <h1>{Name}</h1>
      <p>Generated by lan add page {name_kebab}</p>
    </main>
  );
}
";

const NEXTJS_LAYOUT: &str = "\
import type { ReactNode } from 'react';

export default function {Name}Layout({ children }: { children: ReactNode }) {
  return <section data-layout=\"{name_kebab}\">{children}</section>;
}
";

const NEXTJS_ROUTE: &str = "\
import { NextResponse } from 'next/server';

export async function GET() {
  return NextResponse.json({ resource: '{name_kebab}', items: [] });
}

export async function POST(request: Request) {
  const payload = await request.json().catch(() => null);
  return NextResponse.json({ resource: '{name_kebab}', created: payload }, { status: 201 });
}
";

const NEXTJS_LOADING: &str = "\
export default function {Name}Loading() {
  return <p>Loading {name_kebab}...</p>;
}
";

const NEXTJS_ERROR: &str = "\
'use client';

export default function {Name}Error({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <div>
      <h2>{Name} failed</h2>
      <p>{error.message}</p>
      <button type=\"button\" onClick={reset}>
        Retry
      </button>
    </div>
  );
}
";

const NEXTJS_NOT_FOUND: &str = "\
import Link from 'next/link';

export default function {Name}NotFound() {
  return (
    <div>
      <h2>{Name} not found</h2>
      <Link href=\"/\">Back home</Link>
    </div>
  );
}
";

const NEXTJS_COMPONENT: &str = "\
export interface {Name}Props {
  className?: string;
  children?: React.ReactNode;
}

export function {Name}({ className, children }: {Name}Props) {
  return <div className={className}>{children}</div>;
}

export default {Name};
";

const NEXTJS_HOOK: &str = "\
import { useCallback, useState } from 'react';

export function use{Name}(initial = false) {
  const [value, setValue] = useState(initial);
  const toggle = useCallback(() => setValue((current) => !current), []);
  return { value, toggle, setValue };
}
";

const NEXTJS_MIDDLEWARE: &str = "\
import { NextResponse } from 'next/server';

export function middleware(request: Request) {
  return NextResponse.next({ headers: { 'x-lania-{name_kebab}': '1' } });
}

export const config = {
  matcher: ['/api/:path*'],
};
";

const NEXTJS_ACTION: &str = "\
'use server';

export async function {name_camel}Action(formData: FormData) {
  const name = String(formData.get('name') ?? '');
  return { ok: name.length > 0, name };
}
";

const GIN_MODEL: &str = "\
package model

import \"time\"

// {Name} 对应数据库实体。
type {Name} struct {
\tID        string    `json:\"id\" gorm:\"primaryKey\"`
\tName      string    `json:\"name\"`
\tCreatedAt time.Time `json:\"createdAt\"`
}
";

const GIN_REPOSITORY: &str = "\
package repository

import (
\t\"sync\"

\t\"example.com/project/internal/model\"
)

// {Name}Repository 内存实现：便于生成后即刻可跑，替换为数据库实现即可。
type {Name}Repository struct {
\tmu    sync.RWMutex
\titems map[string]model.{Name}
}

func New{Name}Repository() *{Name}Repository {
\treturn &{Name}Repository{items: make(map[string]model.{Name})}
}

func (r *{Name}Repository) Save(item model.{Name}) model.{Name} {
\tr.mu.Lock()
\tdefer r.mu.Unlock()
\tr.items[item.ID] = item
\treturn item
}

func (r *{Name}Repository) List() []model.{Name} {
\tr.mu.RLock()
\tdefer r.mu.RUnlock()
\titems := make([]model.{Name}, 0, len(r.items))
\tfor _, item := range r.items {
\t\titems = append(items, item)
\t}
\treturn items
}
";

const GIN_SERVICE: &str = "\
package service

import (
\t\"crypto/rand\"
\t\"encoding/hex\"
\t\"time\"

\t\"example.com/project/internal/model\"
\t\"example.com/project/internal/repository\"
)

type {Name}Service struct {
\trepo *repository.{Name}Repository
}

func New{Name}Service(repo *repository.{Name}Repository) *{Name}Service {
\treturn &{Name}Service{repo: repo}
}

func (s *{Name}Service) Create(name string) model.{Name} {
\tbuffer := make([]byte, 8)
\t_, _ = rand.Read(buffer)
\treturn s.repo.Save(model.{Name}{
\t\tID:        hex.EncodeToString(buffer),
\t\tName:      name,
\t\tCreatedAt: time.Now(),
\t})
}

func (s *{Name}Service) List() []model.{Name} {
\treturn s.repo.List()
}
";

const GIN_HANDLER: &str = "\
package handler

import (
\t\"net/http\"

\t\"github.com/gin-gonic/gin\"

\t\"example.com/project/internal/repository\"
\t\"example.com/project/internal/service\"
)

type {Name}Handler struct {
\tservice *service.{Name}Service
}

func New{Name}Handler(svc *service.{Name}Service) *{Name}Handler {
\treturn &{Name}Handler{service: svc}
}

func (h *{Name}Handler) Register(group *gin.RouterGroup) {
\tgroup.GET(\"/{name_kebab}\", h.List)
\tgroup.POST(\"/{name_kebab}\", h.Create)
}

func (h *{Name}Handler) List(ctx *gin.Context) {
\tctx.JSON(http.StatusOK, gin.H{\"items\": h.service.List()})
}

func (h *{Name}Handler) Create(ctx *gin.Context) {
\tvar body struct {
\t\tName string `json:\"name\"`
\t}
\tif err := ctx.ShouldBindJSON(&body); err != nil {
\t\tctx.JSON(http.StatusBadRequest, gin.H{\"error\": err.Error()})
\t\treturn
\t}
\tctx.JSON(http.StatusCreated, h.service.Create(body.Name))
}

// Register{Name} 供 router.go 的 `// <lania:routes>` 锚点调用。
func Register{Name}(group *gin.RouterGroup) {
\tNew{Name}Handler(service.New{Name}Service(repository.New{Name}Repository())).Register(group)
}
";

const GIN_MIDDLEWARE: &str = "\
package middleware

import \"github.com/gin-gonic/gin\"

// {Name} 记录 {name_kebab} 相关请求的出口状态。
func {Name}() gin.HandlerFunc {
\treturn func(ctx *gin.Context) {
\t\tctx.Next()
\t\tctx.Header(\"x-lania-{name_kebab}\", \"1\")
\t}
}
";

const GIN_ROUTER: &str = "\
package router

import (
\t\"github.com/gin-gonic/gin\"

\t\"example.com/project/internal/middleware\"
)

func Register{Name}(engine *gin.Engine) {
\tgroup := engine.Group(\"/{name_kebab}\")
\tgroup.Use(middleware.{Name}())
}
";

const FASTAPI_SCHEMA: &str = "\
from pydantic import BaseModel, Field


class {Name}Base(BaseModel):
    name: str = Field(..., min_length=1, description=\"{Name} name\")


class {Name}Create({Name}Base):
    pass


class {Name}Read({Name}Base):
    id: str
";

const FASTAPI_MODEL: &str = "\
from dataclasses import dataclass, field


@dataclass
class {Name}:
    id: str
    name: str
    tags: list[str] = field(default_factory=list)
";

const FASTAPI_CRUD: &str = "\
from uuid import uuid4

from app.models.{name_snake} import {Name}
from app.schemas.{name_snake} import {Name}Create


_items: dict[str, {Name}] = {}


def create(payload: {Name}Create) -> {Name}:
    item = {Name}(id=str(uuid4()), name=payload.name)
    _items[item.id] = item
    return item


def list_all() -> list[{Name}]:
    return list(_items.values())


def get(item_id: str) -> {Name} | None:
    return _items.get(item_id)
";

const FASTAPI_ROUTER: &str = "\
from fastapi import APIRouter, HTTPException, status

from app.crud import {name_snake} as crud
from app.schemas.{name_snake} import {Name}Create, {Name}Read

router = APIRouter(prefix=\"/{name_kebab}\", tags=[\"{name_kebab}\"])


@router.get(\"\", response_model=list[{Name}Read])
def list_{name_snake}() -> list[{Name}Read]:
    return crud.list_all()


@router.post(\"\", response_model={Name}Read, status_code=status.HTTP_201_CREATED)
def create_{name_snake}(payload: {Name}Create) -> {Name}Read:
    return crud.create(payload)


@router.get(\"/{item_id}\", response_model={Name}Read)
def get_{name_snake}(item_id: str) -> {Name}Read:
    item = crud.get(item_id)
    if item is None:
        raise HTTPException(status_code=404, detail=\"{Name} not found\")
    return item
";

const FASTAPI_DEPENDENCY: &str = "\
from collections.abc import Iterator

from app.crud import {name_snake} as crud


def {name_snake}_repository() -> Iterator[dict[str, object]]:
    yield crud._items
";

const FASTAPI_MIDDLEWARE: &str = "\
from collections.abc import Awaitable, Callable

from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import Response


class {Name}Middleware(BaseHTTPMiddleware):
    async def dispatch(
        self, request: Request, call_next: Callable[[Request], Awaitable[Response]]
    ) -> Response:
        response = await call_next(request)
        response.headers[\"x-lania-{name_kebab}\"] = \"1\"
        return response
";

const AXUM_MODEL: &str = "\
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct {Name} {
    pub id: String,
    pub name: String,
}
";

const AXUM_REPOSITORY: &str = "\
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::model::{name_snake}::{Name};

/// 内存实现：`Clone` 是共享同一份数据（`Arc<RwLock<_>>`），
/// 这样 service 能直接作为 axum 的 `State` 使用，不必改 shared 状态类型。
#[derive(Clone, Default)]
pub struct {Name}Repository {
    items: Arc<RwLock<HashMap<String, {Name}>>>,
}

impl {Name}Repository {
    pub fn save(&self, item: {Name}) -> {Name} {
        self.items
            .write()
            .expect(\"repository lock poisoned\")
            .insert(item.id.clone(), item.clone());
        item
    }

    pub fn list(&self) -> Vec<{Name}> {
        self.items
            .read()
            .expect(\"repository lock poisoned\")
            .values()
            .cloned()
            .collect()
    }
}
";

const AXUM_SERVICE: &str = "\
use crate::model::{name_snake}::{Name};
use crate::repository::{name_snake}::{Name}Repository;

#[derive(Clone, Default)]
pub struct {Name}Service {
    repository: {Name}Repository,
}

impl {Name}Service {
    pub fn create(&self, name: impl Into<String>) -> {Name} {
        let item = {Name} {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
        };
        self.repository.save(item)
    }

    pub fn list(&self) -> Vec<{Name}> {
        self.repository.list()
    }
}
";

const AXUM_HANDLER: &str = "\
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};

use crate::model::{name_snake}::{Name};
use crate::service::{name_snake}::{Name}Service;

pub async fn list_{name_snake}(State(service): State<{Name}Service>) -> impl IntoResponse {
    Json(service.list())
}

pub async fn create_{name_snake}(
    State(service): State<{Name}Service>,
    Json(payload): Json<{Name}>,
) -> impl IntoResponse {
    (StatusCode::CREATED, Json(service.create(payload.name)))
}
";

const AXUM_MIDDLEWARE: &str = "\
use axum::{extract::Request, middleware::Next, response::Response};

pub async fn {name_snake}(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        \"x-lania-{name_kebab}\",
        axum::http::HeaderValue::from_static(\"1\"),
    );
    response
}
";

const AXUM_ROUTER: &str = "\
use axum::{routing::get, Router};

use crate::handler::{name_snake}::{create_{name_snake}, list_{name_snake}};
use crate::service::{name_snake}::{Name}Service;

/// 在 `main.rs` 里 `.merge({name_snake}_router())` 后即可生效。
pub fn {name_snake}_router() -> Router<{Name}Service> {
    Router::new()
        .route(\"/{name_kebab}\", get(list_{name_snake}).post(create_{name_snake}))
}
";

const FLUTTER_MODEL: &str = "\
class {Name} {
  const {Name}({required this.id, required this.name});

  final String id;
  final String name;

  factory {Name}.fromJson(Map<String, dynamic> json) {
    return {Name}(
      id: json['id'] as String? ?? '',
      name: json['name'] as String? ?? '',
    );
  }

  Map<String, dynamic> toJson() => {'id': id, 'name': name};
}
";

const FLUTTER_SERVICE: &str = "\
import '../models/{name_snake}.dart';

class {Name}Service {
  Future<List<{Name}>> fetchAll() async {
    return const <{Name}>[];
  }

  Future<{Name}?> fetchOne(String id) async {
    return null;
  }
}
";

const FLUTTER_REPOSITORY: &str = "\
import '../models/{name_snake}.dart';
import '../services/{name_snake}_service.dart';

class {Name}Repository {
  {Name}Repository(this._service);

  final {Name}Service _service;

  Future<List<{Name}>> list() => _service.fetchAll();
}
";

const FLUTTER_CONTROLLER: &str = "\
import 'package:flutter/foundation.dart';

import '../models/{name_snake}.dart';
import '../services/{name_snake}_service.dart';

class {Name}Controller extends ChangeNotifier {
  {Name}Controller(this._service);

  final {Name}Service _service;
  List<{Name}> items = const [];
  bool loading = false;

  Future<void> load() async {
    loading = true;
    notifyListeners();
    items = await _service.fetchAll();
    loading = false;
    notifyListeners();
  }
}
";

const FLUTTER_PAGE: &str = "\
import 'package:flutter/material.dart';

import '../controllers/{name_snake}_controller.dart';
import '../services/{name_snake}_service.dart';

class {Name}Page extends StatefulWidget {
  const {Name}Page({super.key});

  @override
  State<{Name}Page> createState() => _{Name}PageState();
}

class _{Name}PageState extends State<{Name}Page> {
  late final {Name}Controller _controller;

  @override
  void initState() {
    super.initState();
    _controller = {Name}Controller({Name}Service())..load();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('{Name}')),
      body: AnimatedBuilder(
        animation: _controller,
        builder: (context, _) {
          if (_controller.loading) {
            return const Center(child: CircularProgressIndicator());
          }
          return ListView.builder(
            itemCount: _controller.items.length,
            itemBuilder: (context, index) => ListTile(
              title: Text(_controller.items[index].name),
            ),
          );
        },
      ),
    );
  }
}
";

const FLUTTER_WIDGET: &str = "\
import 'package:flutter/material.dart';

class {Name} extends StatelessWidget {
  const {Name}({super.key, this.child});

  final Widget? child;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(16),
      child: child,
    );
  }
}
";

/// 全部内置模板。
pub const ARTIFACT_TEMPLATES: &[ArtifactTemplate] = &[
    ArtifactTemplate {
        name: "nestjs.module",
        content: NESTJS_MODULE,
    },
    ArtifactTemplate {
        name: "nestjs.controller",
        content: NESTJS_CONTROLLER,
    },
    ArtifactTemplate {
        name: "nestjs.service",
        content: NESTJS_SERVICE,
    },
    ArtifactTemplate {
        name: "nestjs.dto_create",
        content: NESTJS_DTO_CREATE,
    },
    ArtifactTemplate {
        name: "nestjs.dto_update",
        content: NESTJS_DTO_UPDATE,
    },
    ArtifactTemplate {
        name: "nestjs.entity",
        content: NESTJS_ENTITY,
    },
    ArtifactTemplate {
        name: "nestjs.controller_spec",
        content: NESTJS_CONTROLLER_SPEC,
    },
    ArtifactTemplate {
        name: "nestjs.resolver",
        content: NESTJS_RESOLVER,
    },
    ArtifactTemplate {
        name: "nestjs.guard",
        content: NESTJS_GUARD,
    },
    ArtifactTemplate {
        name: "nestjs.single",
        content: NESTJS_SINGLE,
    },
    ArtifactTemplate {
        name: "component.rfc",
        content: COMPONENT_RFC,
    },
    ArtifactTemplate {
        name: "component.rcc",
        content: COMPONENT_RCC,
    },
    ArtifactTemplate {
        name: "component.v2",
        content: COMPONENT_V2,
    },
    ArtifactTemplate {
        name: "component.v3",
        content: COMPONENT_V3,
    },
    ArtifactTemplate {
        name: "component.svelte",
        content: COMPONENT_SVELTE,
    },
    ArtifactTemplate {
        name: "component.astro",
        content: COMPONENT_ASTRO,
    },
    ArtifactTemplate {
        name: "config.eslint",
        content: CONFIG_ESLINT,
    },
    ArtifactTemplate {
        name: "config.prettier",
        content: CONFIG_PRETTIER,
    },
    ArtifactTemplate {
        name: "config.stylelint",
        content: CONFIG_STYLELINT,
    },
    ArtifactTemplate {
        name: "config.tsconfig",
        content: CONFIG_TSCONFIG,
    },
    ArtifactTemplate {
        name: "config.editorconfig",
        content: CONFIG_EDITORCONFIG,
    },
    ArtifactTemplate {
        name: "config.gitignore",
        content: CONFIG_GITIGNORE,
    },
    ArtifactTemplate {
        name: "config.commitizen",
        content: CONFIG_COMMITIZEN,
    },
    ArtifactTemplate {
        name: "nextjs.page",
        content: NEXTJS_PAGE,
    },
    ArtifactTemplate {
        name: "nextjs.layout",
        content: NEXTJS_LAYOUT,
    },
    ArtifactTemplate {
        name: "nextjs.route",
        content: NEXTJS_ROUTE,
    },
    ArtifactTemplate {
        name: "nextjs.loading",
        content: NEXTJS_LOADING,
    },
    ArtifactTemplate {
        name: "nextjs.error",
        content: NEXTJS_ERROR,
    },
    ArtifactTemplate {
        name: "nextjs.not_found",
        content: NEXTJS_NOT_FOUND,
    },
    ArtifactTemplate {
        name: "nextjs.component",
        content: NEXTJS_COMPONENT,
    },
    ArtifactTemplate {
        name: "nextjs.hook",
        content: NEXTJS_HOOK,
    },
    ArtifactTemplate {
        name: "nextjs.middleware",
        content: NEXTJS_MIDDLEWARE,
    },
    ArtifactTemplate {
        name: "nextjs.action",
        content: NEXTJS_ACTION,
    },
    ArtifactTemplate {
        name: "gin.model",
        content: GIN_MODEL,
    },
    ArtifactTemplate {
        name: "gin.repository",
        content: GIN_REPOSITORY,
    },
    ArtifactTemplate {
        name: "gin.service",
        content: GIN_SERVICE,
    },
    ArtifactTemplate {
        name: "gin.handler",
        content: GIN_HANDLER,
    },
    ArtifactTemplate {
        name: "gin.middleware",
        content: GIN_MIDDLEWARE,
    },
    ArtifactTemplate {
        name: "gin.router",
        content: GIN_ROUTER,
    },
    ArtifactTemplate {
        name: "fastapi.schema",
        content: FASTAPI_SCHEMA,
    },
    ArtifactTemplate {
        name: "fastapi.model",
        content: FASTAPI_MODEL,
    },
    ArtifactTemplate {
        name: "fastapi.crud",
        content: FASTAPI_CRUD,
    },
    ArtifactTemplate {
        name: "fastapi.router",
        content: FASTAPI_ROUTER,
    },
    ArtifactTemplate {
        name: "fastapi.dependency",
        content: FASTAPI_DEPENDENCY,
    },
    ArtifactTemplate {
        name: "fastapi.middleware",
        content: FASTAPI_MIDDLEWARE,
    },
    ArtifactTemplate {
        name: "axum.model",
        content: AXUM_MODEL,
    },
    ArtifactTemplate {
        name: "axum.repository",
        content: AXUM_REPOSITORY,
    },
    ArtifactTemplate {
        name: "axum.service",
        content: AXUM_SERVICE,
    },
    ArtifactTemplate {
        name: "axum.handler",
        content: AXUM_HANDLER,
    },
    ArtifactTemplate {
        name: "axum.middleware",
        content: AXUM_MIDDLEWARE,
    },
    ArtifactTemplate {
        name: "axum.router",
        content: AXUM_ROUTER,
    },
    ArtifactTemplate {
        name: "flutter.model",
        content: FLUTTER_MODEL,
    },
    ArtifactTemplate {
        name: "flutter.service",
        content: FLUTTER_SERVICE,
    },
    ArtifactTemplate {
        name: "flutter.repository",
        content: FLUTTER_REPOSITORY,
    },
    ArtifactTemplate {
        name: "flutter.controller",
        content: FLUTTER_CONTROLLER,
    },
    ArtifactTemplate {
        name: "flutter.page",
        content: FLUTTER_PAGE,
    },
    ArtifactTemplate {
        name: "flutter.widget",
        content: FLUTTER_WIDGET,
    },
];

pub fn find_template(name: &str) -> Option<&'static ArtifactTemplate> {
    ARTIFACT_TEMPLATES
        .iter()
        .find(|template| template.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::add::generators::all_generators;

    #[test]
    fn every_generator_artifact_has_a_template() {
        for generator in all_generators() {
            for artifact in generator.artifacts {
                assert!(
                    find_template(artifact.template).is_some(),
                    "generator {} references unknown template {}",
                    generator.name,
                    artifact.template
                );
            }
        }
    }

    #[test]
    fn template_names_are_unique() {
        let names: Vec<&str> = ARTIFACT_TEMPLATES.iter().map(|item| item.name).collect();
        let unique: std::collections::HashSet<&&str> = names.iter().collect();
        assert_eq!(names.len(), unique.len());
    }

    /// 渲染只做 `{key}` 替换，没有转义机制：模板里写成 `{{` / `}}` 会原样进产物。
    ///
    /// 注意 `::{{` 是合法的 Rust 路径加大括号（`path::{{a, b}}` 的紧凑写法），
    /// 所以只断言「双花括号后紧跟花括号内的内容」这一真正的 bug 形态。
    #[test]
    fn templates_do_not_contain_escape_style_doubled_braces() {
        for template in ARTIFACT_TEMPLATES {
            for line in template.content.lines() {
                let doubled_open = line.contains("{{") && !line.contains("::{");
                let doubled_close = line.contains("}}") && !line.contains("}};");
                assert!(
                    !doubled_open && !doubled_close,
                    "template {} contains escape-style doubled braces, which are emitted verbatim: {line:?}",
                    template.name
                );
            }
        }
    }
}

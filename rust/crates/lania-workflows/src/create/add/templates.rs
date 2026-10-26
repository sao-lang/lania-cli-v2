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
}

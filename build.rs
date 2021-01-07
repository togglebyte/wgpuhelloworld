use std::fs::{read_to_string, write};
use std::path::PathBuf;

use anyhow::*;
use glob::glob;
use shaderc::{Compiler, ShaderKind};

struct ShaderData {
    src: String,
    src_path: PathBuf,
    spv_path: PathBuf,
    kind: ShaderKind,
}

impl ShaderData {
    pub fn load(src_path: PathBuf) -> Result<Self> {
        let extension = src_path
            .extension()
            .context("file has no extension")?
            .to_str()
            .context("Extension omg fail")?;

        let kind = match extension {
            "vert" => ShaderKind::Vertex,
            "frag" => ShaderKind::Fragment,
            "comp" => ShaderKind::Compute,
            _ => bail!("no no no"),
        };

        let src = read_to_string(src_path.clone())?;
        let spv_path = src_path.with_extension(format!("{}.spv", extension));
        
        Ok(Self {
            src,
            src_path,
            spv_path,
            kind
        })
    }
}

fn main() -> Result<()> {
    let mut shader_paths = [
        glob("./src/**/*.vert")?,
        glob("./src/**/*.frag")?,
        glob("./src/**/*.comp")?,
    ];


    let shaders = shader_paths
        .iter_mut()
        .flatten()
        .map(|glob_result| ShaderData::load(glob_result?));
    
    let mut compiler = Compiler::new().unwrap();

    for shader in shaders {
        let shader = shader?;
        println!("cargo:rerun-if-changed={:?}", shader.src_path);
        let compiled = compiler.compile_into_spirv(
            &shader.src,
            shader.kind,
            shader.src_path.to_str().unwrap(),
            "main",
            None,
        )?;
        write(shader.spv_path, compiled.as_binary_u8())?;
    }

    Ok(())
}

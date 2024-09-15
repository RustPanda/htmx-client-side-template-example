use std::borrow::Cow;

use rust_embed::RustEmbed;

pub trait IncludeExt<T>
where
    T: RustEmbed,
{
    fn include(&self, file_path: &'static str) -> Cow<'static, str> {
        match T::get(file_path).map(|f| f.data) {
            Some(Cow::Borrowed(bytes)) => {
                // Преобразуем байты в строку
                core::str::from_utf8(bytes)
                    .map(Cow::Borrowed)
                    .unwrap_or_default()
            }
            Some(Cow::Owned(bytes)) => {
                // Преобразуем байты в строку и создаем Cow::Owned
                String::from_utf8(bytes).map(Cow::Owned).unwrap_or_default()
            }
            None => unimplemented!(),
        }
    }

    fn include_template(&self, id: &'static str, file_path: &'static str) -> String {
        format!(
            "<template id=\"{id}\">\n{}\n</template>",
            self.include(file_path)
        )
    }
}

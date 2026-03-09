mod app;
mod backend;
mod proxy;

pub use app::{App, AppBuilder, run};

#[derive(Clone, Copy, Debug)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

#[derive(Clone, Copy, Debug)]
pub struct RouteDef {
    /// Метод (GET/POST/…)
    pub method: Method,
    /// Путь ручки внутри сервиса (относительно prefix), например "/ping" или "/{id}"
    pub path: &'static str,
    /// Куда проксировать внутри сервиса. Если None => to == path.
    ///
    /// В минимальной версии поддерживаются только:
    /// - None  => прокидываем фактический путь после strip(prefix) (работает и с {id})
    /// - Some("/fixed") => фиксированный путь (без подстановки параметров)
    pub to: Option<&'static str>,
}

#[derive(Clone, Copy, Debug)]
pub struct Routes {
    /// имя сервиса (и ключ backend-пула)
    pub service: &'static str,
    /// внешний префикс, например "/sales"
    pub prefix: &'static str,
    /// список ручек
    pub routes: &'static [RouteDef],
}

Name:           telegram-cli
Version:        0.1.0
Release:        1%{?dist}
Summary:        Telegram CLI - terminal Telegram client

License:        MIT
URL:            https://github.com/ZONGRUICHD/Telegram-TUI
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  cmake
BuildRequires:  gcc
BuildRequires:  pkgconfig(tdjson)
Requires:       tdlib

%description
TDLib + Rust daemon + TUI/CLI — a full-featured Telegram client for the
terminal. Features include message management, channel monitoring, file
operations, and a ratatui-powered TUI interface.

%prep
%autosetup -n Telegram-TUI-%{version}

%build
cargo build --release

%install
install -Dm755 target/release/tg   %{buildroot}%{_bindir}/tg
install -Dm755 target/release/tgcd %{buildroot}%{_bindir}/tgcd
install -Dm644 scripts/tgcd.service %{buildroot}%{_userunitdir}/tgcd.service
install -Dm644 LICENSE %{buildroot}%{_docdir}/%{name}/LICENSE
install -Dm644 README.md %{buildroot}%{_docdir}/%{name}/README.md
install -Dm644 docs/tutorial.md %{buildroot}%{_docdir}/%{name}/tutorial.md

%files
%license LICENSE
%doc README.md docs/tutorial.md
%{_bindir}/tg
%{_bindir}/tgcd
%{_userunitdir}/tgcd.service

%changelog
* Sun Jun 15 2026 zong1024 <zong1024@gmail.com> - 0.1.0-1
- Initial package
